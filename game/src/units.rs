use crate::components::*;
use crate::fx::FxEvent;
use crate::models::ModelKit;
use crate::terrain;
use crate::world::spawn_unit;
use bevy::light::NotShadowCaster;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy)]
pub struct Snap {
    /// Position am Boden (inkl. Geländehöhe)
    pub pos: Vec3,
    pub team: Team,
    pub radius: f32,
    pub is_building: bool,
}

/// Positionen aller kampffähigen Objekte dieses Frames
#[derive(Resource, Default)]
pub struct Snapshot(pub HashMap<Entity, Snap>);

fn dist2d(a: Vec3, b: Vec3) -> f32 {
    flat(a).distance(flat(b))
}

/// Winkel b - a im Bereich [-PI, PI]
fn angle_diff(a: f32, b: f32) -> f32 {
    (b - a + PI).rem_euclid(TAU) - PI
}

pub fn snapshot(
    mut snap: ResMut<Snapshot>,
    q: Query<(Entity, &Transform, &Team, Option<&Unit>, Option<&Building>), With<Health>>,
) {
    snap.0.clear();
    for (e, t, team, u, b) in &q {
        snap.0.insert(
            e,
            Snap {
                pos: t.translation,
                team: *team,
                radius: u.map(|u| u.radius).or(b.map(|b| b.radius)).unwrap_or(1.0),
                is_building: b.is_some(),
            },
        );
    }
}

/// Bewaffnete Einheiten suchen sich selbst Ziele, wenn sie nicht gerade einen Marschbefehl haben
pub fn auto_target(
    mut commands: Commands,
    snap: Res<Snapshot>,
    q: Query<(Entity, &Weapon, &Team, Has<MoveTo>, Has<AttackMove>), Without<AttackTarget>>,
) {
    for (e, w, team, moving, amove) in &q {
        if moving && !amove {
            continue;
        }
        let Some(me) = snap.0.get(&e) else { continue };
        let sight = w.range + 6.0;
        let mut best: Option<(Entity, f32)> = None;
        for (oe, o) in &snap.0 {
            if o.team == *team {
                continue;
            }
            // Einheiten vor Gebäuden bevorzugen
            let d = dist2d(me.pos, o.pos) - o.radius + if o.is_building { 4.0 } else { 0.0 };
            if d < sight && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((*oe, d));
            }
        }
        if let Some((t, _)) = best {
            commands.entity(e).insert(AttackTarget(t));
        }
    }
}

/// Zielhöhe für Geschosse (Mitte des Ziels)
fn aim_point(s: &Snap) -> Vec3 {
    s.pos + Vec3::Y * if s.is_building { 2.2 } else { 0.8 }
}

pub fn combat(
    mut commands: Commands,
    time: Res<Time>,
    snap: Res<Snapshot>,
    kit: Res<ModelKit>,
    mut fx: MessageWriter<FxEvent>,
    mut q: Query<(Entity, &mut Weapon, &AttackTarget, &Team, &Transform, &Unit)>,
) {
    let dt = time.delta_secs();
    for (e, mut w, target, team, t, unit) in &mut q {
        w.timer = (w.timer - dt).max(0.0);
        let Some(tgt) = snap.0.get(&target.0) else {
            commands.entity(e).remove::<AttackTarget>();
            continue;
        };
        let to = flat(tgt.pos - t.translation);
        let dist = to.length() - tgt.radius;
        if dist > w.range || w.timer > 0.0 {
            continue;
        }
        w.timer = w.cooldown;
        let dir = to.normalize_or_zero();
        let (muzzle_len, muzzle_h) = if unit.kind == UnitKind::Werfer { (1.0, 1.5) } else { (2.2, 1.05) };
        let from = t.translation + Vec3::Y * muzzle_h + dir * muzzle_len;
        let vel = (aim_point(tgt) - from).normalize_or_zero();
        let t_idx = team.0 as usize;
        let (mat, scale) = if w.rocket {
            (kit.rocket_mat[t_idx].clone(), Vec3::new(2.4, 0.8, 2.4))
        } else {
            (kit.tracer_mat[t_idx].clone(), Vec3::ONE)
        };
        commands.spawn((
            Mesh3d(kit.tracer.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(from)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, vel))
                .with_scale(scale),
            NotShadowCaster,
            Projectile {
                target: target.0,
                damage: w.damage,
                speed: w.projectile_speed,
                rocket: w.rocket,
                smoke_timer: 0.0,
            },
        ));
        if !w.rocket {
            fx.write(FxEvent::Muzzle { pos: from, dir: vel });
        } else {
            fx.write(FxEvent::Launch { pos: from, dir: vel });
        }
    }
}

pub fn movement(
    mut commands: Commands,
    time: Res<Time>,
    snap: Res<Snapshot>,
    mut q: Query<(
        Entity,
        &Unit,
        &mut Transform,
        &mut Heading,
        Option<&MoveTo>,
        Option<&AttackTarget>,
        Option<&Weapon>,
    )>,
) {
    let dt = time.delta_secs();
    for (e, unit, mut t, mut heading, mv, atk, w) in &mut q {
        let me = flat(t.translation);
        let mut goal: Option<(Vec3, f32)> = None;
        let mut engaged = false;
        if let (Some(a), Some(w)) = (atk, w)
            && let Some(tgt) = snap.0.get(&a.0) {
                engaged = true;
                if dist2d(me, tgt.pos) - tgt.radius > w.range * 0.95 {
                    goal = Some((flat(tgt.pos), 0.0));
                }
            }
        if !engaged
            && let Some(m) = mv {
                goal = Some((flat(m.0), 0.6));
            }
        let Some((g, arrive)) = goal else { continue };
        let to = g - me;
        let d = to.length();
        if d <= arrive.max(0.05) {
            commands.entity(e).remove::<MoveTo>().remove::<AttackMove>();
            continue;
        }
        let dir = to / d;
        // erst drehen, dann fahren: Kettenfahrzeuge wenden auf der Stelle
        let want = Heading::from_dir(dir).0;
        let diff = angle_diff(heading.0, want);
        heading.0 += diff.clamp(-3.2 * dt, 3.2 * dt);
        let align = (1.0 - diff.abs() / 1.2).clamp(0.15, 1.0);
        let step = (unit.speed * align * dt).min(d);
        t.translation += dir * step;
    }
}

/// Einheiten schieben sich auseinander und bleiben außerhalb von Gebäuden
pub fn separation(snap: Res<Snapshot>, mut q: Query<(Entity, &Unit, &mut Transform)>) {
    let list: Vec<(Entity, Snap)> = snap.0.iter().map(|(e, s)| (*e, *s)).collect();
    for (e, unit, mut t) in &mut q {
        let me = flat(t.translation);
        let mut push = Vec3::ZERO;
        for (oe, o) in &list {
            if *oe == e {
                continue;
            }
            let min = unit.radius + o.radius;
            let diff = me - flat(o.pos);
            let d = diff.length();
            if d < min && d > 0.001 {
                let strength = if o.is_building { 1.0 } else { 0.5 };
                push += diff / d * (min - d) * strength;
            } else if d <= 0.001 {
                push += Vec3::new(0.3, 0.0, 0.1);
            }
        }
        t.translation += push.clamp_length_max(0.5);
        t.translation.x = t.translation.x.clamp(-MAP_HALF, MAP_HALF);
        t.translation.z = t.translation.z.clamp(-MAP_HALF, MAP_HALF);
    }
}

/// Fahrzeuge folgen dem Gelände: Höhe und Neigung
pub fn ground_snap(time: Res<Time>, mut q: Query<(&mut Transform, &Heading), With<Unit>>) {
    let k = (10.0 * time.delta_secs()).min(1.0);
    for (mut t, heading) in &mut q {
        let (x, z) = (t.translation.x, t.translation.z);
        t.translation.y = terrain::height(x, z);
        let n = terrain::normal(x, z);
        let target = Quat::from_rotation_arc(Vec3::Y, n) * Quat::from_rotation_y(heading.0);
        t.rotation = t.rotation.slerp(target, k);
    }
}

/// Türme drehen sich zum Ziel, sonst zurück nach vorne
pub fn turret_aim(
    time: Res<Time>,
    snap: Res<Snapshot>,
    units: Query<(&Transform, &Heading, &TurretRef, Option<&AttackTarget>), With<Unit>>,
    mut turrets: Query<(&mut Transform, &Turret), Without<Unit>>,
) {
    let dt = time.delta_secs();
    for (t, heading, tref, atk) in &units {
        let Ok((mut tt, turret)) = turrets.get_mut(tref.0) else { continue };
        let mut local = 0.0;
        if let Some(tgt) = atk.and_then(|a| snap.0.get(&a.0)) {
            let to = flat(tgt.pos - t.translation);
            if to.length_squared() > 0.01 {
                local = angle_diff(heading.0, Heading::from_dir(to).0);
            }
        }
        let (cur, _, _) = tt.rotation.to_euler(EulerRot::YXZ);
        let next = cur + angle_diff(cur, local).clamp(-2.5 * dt, 2.5 * dt);
        tt.rotation = Quat::from_euler(EulerRot::YXZ, next, turret.tilt, 0.0);
        tt.translation = Vec3::new(0.0, turret.base_y, turret.base_z);
    }
}

pub fn harvest(
    mut commands: Commands,
    time: Res<Time>,
    mut stock: ResMut<Stock>,
    mut harvesters: Query<(Entity, &mut Harvester, &Transform, &Team, Has<MoveTo>), Without<ResourceNode>>,
    mut nodes: Query<(Entity, &Transform, &mut ResourceNode)>,
) {
    let dt = time.delta_secs();
    for (e, mut h, t, team, moving) in &mut harvesters {
        let me = flat(t.translation);
        let hq = if *team == PLAYER { PLAYER_HQ_POS } else { ENEMY_HQ_POS };
        match h.state {
            HarvestState::Idle => {
                if moving {
                    continue;
                }
                if h.cargo > 0 {
                    h.state = HarvestState::Returning;
                    continue;
                }
                // Nächstes Feld automatisch anfahren. Bevorzugt wird der knappere Rohstoff,
                // weiter entfernte Felder bekommen einen Aufschlag.
                let s = stock.0[team.0 as usize];
                let want = if s[1] * 2 < s[0] { ResKind::Plasma } else { ResKind::Ferrit };
                let mut best: Option<(Entity, f32)> = None;
                for (ne, nt, n) in &nodes {
                    if n.amount == 0 {
                        continue;
                    }
                    let penalty = if n.kind == want { 0.0 } else { 35.0 };
                    let d = flat(nt.translation).distance(me) + penalty;
                    if best.is_none_or(|(_, bd)| d < bd) {
                        best = Some((ne, d));
                    }
                }
                if let Some((ne, _)) = best {
                    h.node = Some(ne);
                    h.state = HarvestState::ToNode;
                }
            }
            HarvestState::ToNode => {
                let Some(ne) = h.node else {
                    h.state = HarvestState::Idle;
                    continue;
                };
                let Ok((_, nt, n)) = nodes.get(ne) else {
                    h.node = None;
                    h.state = HarvestState::Idle;
                    continue;
                };
                if n.amount == 0 {
                    h.node = None;
                    h.state = HarvestState::Idle;
                    continue;
                }
                let np = flat(nt.translation);
                if me.distance(np) < 3.4 {
                    commands.entity(e).remove::<MoveTo>();
                    h.state = HarvestState::Mining(0.0);
                } else {
                    commands.entity(e).insert(MoveTo(np));
                }
            }
            HarvestState::Mining(elapsed) => {
                let elapsed = elapsed + dt;
                if elapsed < HARVEST_TIME {
                    h.state = HarvestState::Mining(elapsed);
                    continue;
                }
                let Some(ne) = h.node else {
                    h.state = HarvestState::Idle;
                    continue;
                };
                if let Ok((_, _, mut n)) = nodes.get_mut(ne) {
                    let take = HARVEST_LOAD.min(n.amount);
                    n.amount -= take;
                    h.cargo = take;
                    h.cargo_kind = n.kind;
                    if n.amount == 0 {
                        commands.entity(ne).try_despawn();
                    }
                }
                h.state = HarvestState::Returning;
            }
            HarvestState::Returning => {
                if me.distance(flat(hq)) < 6.6 {
                    stock.0[team.0 as usize][h.cargo_kind.idx()] += h.cargo;
                    h.cargo = 0;
                    commands.entity(e).remove::<MoveTo>();
                    h.state = if h.node.is_some() { HarvestState::ToNode } else { HarvestState::Idle };
                } else {
                    commands.entity(e).insert(MoveTo(hq));
                }
            }
        }
    }
}

pub fn projectiles(
    mut commands: Commands,
    time: Res<Time>,
    snap: Res<Snapshot>,
    mut fx: MessageWriter<FxEvent>,
    mut q: Query<(Entity, &mut Transform, &mut Projectile)>,
    mut health: Query<&mut Health>,
) {
    let dt = time.delta_secs();
    for (e, mut t, mut p) in &mut q {
        let Some(tgt) = snap.0.get(&p.target) else {
            commands.entity(e).try_despawn();
            continue;
        };
        let aim = aim_point(tgt);
        let to = aim - t.translation;
        let d = to.length();
        let step = p.speed * dt;
        if d <= step + tgt.radius * 0.4 {
            if let Ok(mut h) = health.get_mut(p.target) {
                h.hp -= p.damage;
            }
            fx.write(FxEvent::Impact { pos: t.translation, rocket: p.rocket });
            commands.entity(e).try_despawn();
            continue;
        }
        let dir = to / d;
        t.translation += dir * step;
        t.rotation = Quat::from_rotation_arc(Vec3::Y, dir);
        if p.rocket {
            p.smoke_timer -= dt;
            if p.smoke_timer <= 0.0 {
                p.smoke_timer = 0.05;
                fx.write(FxEvent::Trail(t.translation - dir * 0.6));
            }
        }
    }
}

pub fn deaths(
    mut commands: Commands,
    mut fx: MessageWriter<FxEvent>,
    q: Query<(Entity, &Health, &Transform, Has<Building>, Option<&Unit>, Option<&Team>)>,
) {
    for (e, h, t, building, unit, team) in &q {
        if h.hp <= 0.0 {
            let wreck = unit.zip(team).map(|(u, team)| crate::fx::WreckInfo { kind: u.kind, team: *team, rot: t.rotation });
            fx.write(FxEvent::Death { pos: t.translation, big: building, wreck });
            commands.entity(e).try_despawn();
        }
    }
}

pub fn check_outcome(mut outcome: ResMut<Outcome>, hqs: Query<&Team, With<Hq>>) {
    if *outcome != Outcome::Running {
        return;
    }
    let player = hqs.iter().any(|t| *t == PLAYER);
    let enemy = hqs.iter().any(|t| *t == ENEMY);
    if !player {
        *outcome = Outcome::Lost;
    } else if !enemy {
        *outcome = Outcome::Won;
    }
}

/// Bauaufträge an ein HQ (vom Spieler per Taste/Knopf oder von der KI)
#[derive(Message)]
pub struct BuildRequest {
    pub team: Team,
    pub kind: UnitKind,
}

pub const MAX_QUEUE: usize = 5;

pub fn enqueue_builds(
    mut reqs: MessageReader<BuildRequest>,
    mut stock: ResMut<Stock>,
    mut hqs: Query<(&Team, &mut Hq)>,
) {
    for r in reqs.read() {
        for (team, mut hq) in &mut hqs {
            if *team != r.team || hq.queue.len() >= MAX_QUEUE {
                continue;
            }
            let cost = r.kind.cost();
            if stock.can_afford(r.team, cost) {
                stock.pay(r.team, cost);
                hq.queue.push(r.kind);
            }
        }
    }
}

pub fn production(
    mut commands: Commands,
    time: Res<Time>,
    kit: Res<ModelKit>,
    mut hqs: Query<(&Team, &mut Hq, &Transform)>,
) {
    for (team, mut hq, t) in &mut hqs {
        let Some(&kind) = hq.queue.first() else {
            hq.progress = 0.0;
            continue;
        };
        hq.progress += time.delta_secs();
        if hq.progress >= kind.build_time() {
            hq.progress = 0.0;
            hq.queue.remove(0);
            let dir = if *team == PLAYER { Vec3::new(1.0, 0.0, -1.0) } else { Vec3::new(-1.0, 0.0, 1.0) }.normalize();
            let spawn = flat(t.translation) + dir * 7.0;
            let e = spawn_unit(&mut commands, &kit, *team, kind, spawn);
            if kind != UnitKind::Sammler {
                commands.entity(e).insert(MoveTo(spawn + dir * 6.0));
            }
        }
    }
}
