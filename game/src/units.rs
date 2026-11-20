use crate::components::*;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

#[derive(Clone, Copy)]
pub struct Snap {
    pub pos: Vec3,
    pub team: Team,
    pub radius: f32,
    pub is_building: bool,
}

/// Positionen aller kampffähigen Objekte dieses Frames
#[derive(Resource, Default)]
pub struct Snapshot(pub HashMap<Entity, Snap>);

pub fn snapshot(
    mut snap: ResMut<Snapshot>,
    q: Query<(Entity, &Transform, &Team, Option<&Unit>, Option<&Building>), With<Health>>,
) {
    snap.0.clear();
    for (e, t, team, u, b) in &q {
        snap.0.insert(
            e,
            Snap {
                pos: flat(t.translation),
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
            let d = me.pos.distance(o.pos) - o.radius + if o.is_building { 4.0 } else { 0.0 };
            if d < sight && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((*oe, d));
            }
        }
        if let Some((t, _)) = best {
            commands.entity(e).insert(AttackTarget(t));
        }
    }
}

pub fn combat(
    mut commands: Commands,
    time: Res<Time>,
    snap: Res<Snapshot>,
    assets: Res<Assets3d>,
    mut q: Query<(Entity, &mut Weapon, &AttackTarget, &Team, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (e, mut w, target, team, mut t) in &mut q {
        w.timer = (w.timer - dt).max(0.0);
        let Some(tgt) = snap.0.get(&target.0) else {
            commands.entity(e).remove::<AttackTarget>();
            continue;
        };
        let me = flat(t.translation);
        let to = tgt.pos - me;
        let dist = to.length() - tgt.radius;
        if dist <= w.range {
            if to.length_squared() > 0.01 {
                let look = me + to;
                let y = t.translation.y;
                t.look_at(Vec3::new(look.x, y, look.z), Vec3::Y);
            }
            if w.timer <= 0.0 {
                w.timer = w.cooldown;
                commands.spawn((
                    Mesh3d(assets.projectile.clone()),
                    MeshMaterial3d(assets.projectile_mat[team.0 as usize].clone()),
                    Transform::from_translation(t.translation + Vec3::Y * 0.8),
                    Projectile {
                        target: target.0,
                        damage: w.damage,
                        speed: w.projectile_speed,
                    },
                ));
            }
        }
    }
}

pub fn movement(
    mut commands: Commands,
    time: Res<Time>,
    snap: Res<Snapshot>,
    mut q: Query<(Entity, &Unit, &mut Transform, Option<&MoveTo>, Option<&AttackTarget>, Option<&Weapon>)>,
) {
    let dt = time.delta_secs();
    for (e, unit, mut t, mv, atk, w) in &mut q {
        let me = flat(t.translation);
        let mut goal: Option<(Vec3, f32)> = None;
        let mut engaged = false;
        if let (Some(a), Some(w)) = (atk, w)
            && let Some(tgt) = snap.0.get(&a.0) {
                engaged = true;
                if me.distance(tgt.pos) - tgt.radius > w.range * 0.95 {
                    goal = Some((tgt.pos, 0.0));
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
        let step = (unit.speed * dt).min(d);
        let dir = to / d;
        t.translation += dir * step;
        let target_rot = Transform::from_translation(t.translation)
            .looking_to(dir, Vec3::Y)
            .rotation;
        t.rotation = t.rotation.slerp(target_rot, (8.0 * dt).min(1.0));
    }
}

/// Einheiten schieben sich auseinander und bleiben außerhalb von Gebäuden
pub fn separation(
    snap: Res<Snapshot>,
    mut q: Query<(Entity, &Unit, &mut Transform)>,
) {
    let list: Vec<(Entity, Snap)> = snap.0.iter().map(|(e, s)| (*e, *s)).collect();
    for (e, unit, mut t) in &mut q {
        let me = flat(t.translation);
        let mut push = Vec3::ZERO;
        for (oe, o) in &list {
            if *oe == e {
                continue;
            }
            let min = unit.radius + o.radius;
            let diff = me - o.pos;
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
                if me.distance(np) < 3.2 {
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
                if me.distance(hq) < 6.0 {
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
    mut q: Query<(Entity, &mut Transform, &Projectile)>,
    mut health: Query<&mut Health>,
) {
    let dt = time.delta_secs();
    for (e, mut t, p) in &mut q {
        let Some(tgt) = snap.0.get(&p.target) else {
            commands.entity(e).try_despawn();
            continue;
        };
        let aim = tgt.pos + Vec3::Y * if tgt.is_building { 2.0 } else { 0.6 };
        let to = aim - t.translation;
        let d = to.length();
        let step = p.speed * dt;
        if d <= step + tgt.radius * 0.5 {
            if let Ok(mut h) = health.get_mut(p.target) {
                h.hp -= p.damage;
            }
            commands.entity(e).try_despawn();
        } else {
            t.translation += to / d * step;
        }
    }
}

pub fn deaths(
    mut commands: Commands,
    q: Query<(Entity, &Health)>,
) {
    for (e, h) in &q {
        if h.hp <= 0.0 {
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
    assets: Res<Assets3d>,
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
            let dir = if *team == PLAYER { Vec3::new(1.0, 0.0, -1.0) } else { Vec3::new(-1.0, 0.0, 1.0) };
            let spawn = flat(t.translation) + dir * 5.5;
            let e = spawn_unit(&mut commands, &assets, *team, kind, spawn);
            if kind != UnitKind::Sammler {
                commands.entity(e).insert(MoveTo(spawn + dir * 6.0));
            }
        }
    }
}

use crate::world::spawn_unit;
