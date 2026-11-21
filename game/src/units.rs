use crate::components::*;
use crate::nav::{NavGrid, Path, PathQueue};
use crate::fx::FxEvent;
use crate::models::ModelKit;
use crate::orders::{Alerts, GroupSpeed, HoldPosition, Rally};
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
    /// Mauer oder Tor: nur auf Befehl oder beim Durchbruch ein Ziel
    pub wall: bool,
    pub gate: bool,
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
    q: Query<(Entity, &Transform, &Team, Option<&Unit>, Option<&Building>, Has<Wall>, Has<Gate>), With<Health>>,
) {
    snap.0.clear();
    for (e, t, team, u, b, wall, gate) in &q {
        snap.0.insert(
            e,
            Snap {
                pos: t.translation,
                team: *team,
                radius: u.map(|u| u.radius).or(b.map(|b| b.radius)).unwrap_or(1.0),
                is_building: b.is_some(),
                wall,
                gate,
            },
        );
    }
}

/// Bewaffnete Einheiten suchen sich selbst Ziele, wenn sie nicht gerade einen Marschbefehl haben
pub fn auto_target(
    mut commands: Commands,
    snap: Res<Snapshot>,
    q: Query<(Entity, &Weapon, &Team, Has<MoveTo>, Has<AttackMove>, Has<HoldPosition>), (Without<AttackTarget>, Without<Construction>)>,
    fog: Option<Res<crate::fog::FogGrid>>,
) {
    for (e, w, team, moving, amove, hold) in &q {
        if moving && !amove {
            continue;
        }
        let Some(me) = snap.0.get(&e) else { continue };
        // wer die Stellung hält, sucht nur Ziele in Schussweite
        let sight = if hold { w.range } else { w.range + 6.0 };
        let mut best: Option<(Entity, f32)> = None;
        for (oe, o) in &snap.0 {
            if o.team == *team || o.wall {
                continue;
            }
            // Der Spieler schießt nur auf das, was er sieht (Artillerie braucht Aufklärung)
            if *team == PLAYER && fog.as_ref().is_some_and(|f| !f.visible(o.pos)) {
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
    mut q: Query<(Entity, &mut Weapon, &AttackTarget, &Team, &Transform, Option<&Unit>, Option<&mut Motion>, Option<&Structure>), Without<Construction>>,
) {
    let dt = time.delta_secs();
    for (e, mut w, target, team, t, unit, motion, structure) in &mut q {
        w.timer = (w.timer - dt).max(0.0);
        let Some(tgt) = snap.0.get(&target.0) else {
            commands.entity(e).remove::<AttackTarget>();
            continue;
        };
        let to = flat(tgt.pos - t.translation);
        let dist = to.length() - tgt.radius;
        // Geschütztürme können nicht nachfahren: Ziel außer Reichweite aufgeben
        if unit.is_none() && dist > w.range + 1.0 {
            commands.entity(e).remove::<AttackTarget>();
            continue;
        }
        if dist > w.range || w.timer > 0.0 {
            continue;
        }
        w.timer = w.cooldown;
        let dir = to.normalize_or_zero();
        let (muzzle_len, muzzle_h) = match (unit, structure) {
            (Some(u), _) => u.kind.muzzle(),
            // Raketenturm: aus den Behältern seitlich am Kopf
            (None, Some(s)) if s.0 == BuildingKind::Raketenturm => (1.0, 2.6),
            _ => (3.2, 2.1),
        };
        if let Some(mut m) = motion {
            m.recoil = 1.0;
        }
        let from = t.translation + Vec3::Y * muzzle_h + dir * muzzle_len;
        let aim = aim_point(tgt);
        let vel = (aim - from).normalize_or_zero();
        let t_idx = team.0 as usize;
        let caliber = if w.rocket { Caliber::Rocket } else { unit.map_or(Caliber::Medium, |u| u.kind.caliber()) };
        // Geschossform je Kaliber: dünne Leuchtspur bis dicke Granate
        let (mat, scale) = match caliber {
            Caliber::Rocket => (kit.rocket_mat[t_idx].clone(), Vec3::new(2.4, 0.8, 2.4)),
            Caliber::Light => (kit.tracer_mat[t_idx].clone(), Vec3::new(0.6, 0.75, 0.6)),
            Caliber::Medium => (kit.tracer_mat[t_idx].clone(), Vec3::ONE),
            Caliber::Heavy => (kit.tracer_mat[t_idx].clone(), Vec3::new(1.5, 1.25, 1.5)),
            Caliber::Artillery => (kit.tracer_mat[t_idx].clone(), Vec3::new(2.0, 0.8, 2.0)),
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
                team: *team,
                splash: w.splash,
                arc: w.arc,
                caliber,
                base: from,
                traveled: 0.0,
                total: from.distance(aim).max(1.0),
            },
        ));
        if !w.rocket {
            fx.write(FxEvent::Muzzle { pos: from, dir: vel, caliber });
        } else {
            fx.write(FxEvent::Launch { pos: from, dir: vel });
        }
    }
}

pub fn movement(
    mut commands: Commands,
    time: Res<Time>,
    snap: Res<Snapshot>,
    (grid, mut queue): (Option<Res<NavGrid>>, Option<ResMut<PathQueue>>),
    mut q: Query<(
        Entity,
        &Unit,
        &mut Transform,
        &mut Heading,
        Option<&MoveTo>,
        Option<&AttackTarget>,
        Option<&Weapon>,
        Option<&GroupSpeed>,
        Has<HoldPosition>,
        Option<&mut Path>,
    )>,
) {
    let dt = time.delta_secs();
    for (e, unit, mut t, mut heading, mv, atk, w, group, hold, mut path) in &mut q {
        let me = flat(t.translation);
        let mut goal: Option<(Vec3, f32)> = None;
        let mut engaged = false;
        if let (Some(a), Some(w)) = (atk, w)
            && let Some(tgt) = snap.0.get(&a.0) {
                engaged = true;
                let out_of_range = dist2d(me, tgt.pos) - tgt.radius > w.range * 0.95;
                if out_of_range && hold {
                    // Stellung halten: nicht nachfahren, Ziel aufgeben
                    commands.entity(e).remove::<AttackTarget>();
                } else if out_of_range {
                    goal = Some((flat(tgt.pos), 0.0));
                }
            }
        if !engaged
            && let Some(m) = mv {
                goal = Some((flat(m.0), 0.6));
            }
        let Some((g, arrive)) = goal else { continue };
        let d = (g - me).length();
        if d <= arrive.max(0.05) {
            commands.entity(e).remove::<(MoveTo, AttackMove, GroupSpeed)>();
            continue;
        }
        // Wegfindung: der nächsten Ecke des Weges folgen; ohne Weg geradeaus, bis er berechnet ist
        let mut steer = g;
        if let (Some(grid), Some(queue)) = (grid.as_deref(), queue.as_deref_mut()) {
            let tolerance = if engaged { 3.0 } else { 1.0 };
            match path.as_deref_mut() {
                Some(p) if flat(p.goal - g).length() <= tolerance => {
                    if p.version != grid.version {
                        queue.request(e, g);
                    }
                    while p.corners.len() > 1 && flat(p.corners[0] - me).length() < 1.6 {
                        p.corners.remove(0);
                        p.best = f32::MAX;
                        p.stuck = 0.0;
                    }
                    if let Some(c) = p.corners.first() {
                        steer = flat(*c);
                    }
                    let dc = (steer - me).length();
                    // Ziel unerreichbar: am nächstgelegenen Punkt anhalten
                    if !p.reached && !engaged && p.corners.len() <= 1 && dc < 1.2 {
                        commands.entity(e).remove::<(MoveTo, AttackMove, GroupSpeed)>();
                        continue;
                    }
                    // festgefahren? dann neu planen
                    if dc < p.best - 0.3 {
                        p.best = dc;
                        p.stuck = 0.0;
                    } else {
                        p.stuck += dt;
                        if p.stuck > 1.5 {
                            p.stuck = 0.0;
                            p.best = dc;
                            queue.request(e, g);
                        }
                    }
                }
                _ => queue.request(e, g),
            }
        }
        let to = steer - me;
        let ds = to.length().max(1e-3);
        let dir = to / ds;
        // erst drehen, dann fahren: Kettenfahrzeuge wenden auf der Stelle
        let want = Heading::from_dir(dir).0;
        let diff = angle_diff(heading.0, want);
        heading.0 += diff.clamp(-3.2 * dt, 3.2 * dt);
        let align = (1.0 - diff.abs() / 1.2).clamp(0.15, 1.0);
        // Gruppen fahren im Tempo der langsamsten Einheit (nicht im Gefecht)
        let speed = match group {
            Some(g) if !engaged => unit.speed.min(g.0),
            _ => unit.speed,
        };
        let step = (speed * align * dt).min(ds);
        t.translation += dir * step;
    }
}

/// Einheiten schieben sich auseinander und bleiben außerhalb von Gebäuden
pub fn separation(snap: Res<Snapshot>, grid: Option<Res<NavGrid>>, mut q: Query<(Entity, &Unit, &Team, &mut Transform)>) {
    let list: Vec<(Entity, Snap)> = snap.0.iter().map(|(e, s)| (*e, *s)).collect();
    for (e, unit, team, mut t) in &mut q {
        let me = flat(t.translation);
        let mut push = Vec3::ZERO;
        for (oe, o) in &list {
            // eigene Tore sind offen
            if *oe == e || (o.gate && o.team == *team) {
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
        let push = push.clamp_length_max(0.5);
        // nicht in Mauern oder Hänge drücken lassen: sonst nur entlang einer Achse oder gar nicht
        let step = match grid.as_deref() {
            Some(g) => {
                let need = crate::nav::need(unit.radius);
                let here = g.clearance(*team, t.translation);
                let ok = |p: Vec3| {
                    let c = g.clearance(*team, p);
                    c >= need || c >= here
                };
                [push, Vec3::new(push.x, 0.0, 0.0), Vec3::new(0.0, 0.0, push.z)]
                    .into_iter()
                    .find(|s| ok(t.translation + *s))
                    .unwrap_or(Vec3::ZERO)
            }
            None => push,
        };
        t.translation += step;
        t.translation.x = t.translation.x.clamp(-MAP_HALF, MAP_HALF);
        t.translation.z = t.translation.z.clamp(-MAP_HALF, MAP_HALF);
    }
}

/// Fahrzeuge folgen dem Gelände: Höhe und Neigung
pub fn ground_snap(time: Res<Time>, mut q: Query<(&mut Transform, &Heading, &Motion), With<Unit>>) {
    let k = (10.0 * time.delta_secs()).min(1.0);
    for (mut t, heading, motion) in &mut q {
        let (x, z) = (t.translation.x, t.translation.z);
        t.translation.y = terrain::height(x, z);
        let n = terrain::normal(x, z);
        // Federung: beim Schuss hebt sich der Bug kurz, beim Anfahren senkt sich das Heck
        let pitch = motion.recoil * motion.recoil * 0.05 + (motion.speed * 0.006).clamp(-0.03, 0.03);
        let target = Quat::from_rotation_arc(Vec3::Y, n) * Quat::from_rotation_y(heading.0) * Quat::from_rotation_x(pitch);
        t.rotation = t.rotation.slerp(target, k);
    }
}

/// Türme drehen sich zum Ziel, sonst zurück nach vorne
pub fn turret_aim(
    time: Res<Time>,
    snap: Res<Snapshot>,
    units: Query<(&Transform, &Heading, &TurretRef, Option<&AttackTarget>, &Motion), With<Unit>>,
    mut turrets: Query<(&mut Transform, &Turret), Without<Unit>>,
) {
    let dt = time.delta_secs();
    for (t, heading, tref, atk, motion) in &units {
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
        // Rückstoß: Aufbau ruckt nach hinten und kippt kurz nach oben
        let r = motion.recoil * motion.recoil;
        tt.rotation = Quat::from_euler(EulerRot::YXZ, next, turret.tilt + r * 0.06, 0.0);
        tt.translation = Vec3::new(0.0, turret.base_y, turret.base_z) + Quat::from_rotation_y(next) * Vec3::Z * r * 0.16;
    }
}

pub fn harvest(
    mut commands: Commands,
    time: Res<Time>,
    mut stock: ResMut<Stock>,
    mut harvesters: Query<(Entity, &mut Harvester, &Transform, &Team, Has<MoveTo>), Without<ResourceNode>>,
    mut nodes: Query<(Entity, &Transform, &mut ResourceNode)>,
    depots: Query<(&Transform, &Team, &Building), (With<Depot>, Without<Construction>)>,
    (cfg, mut stats): (Option<Res<crate::frame::MatchConfig>>, Option<ResMut<crate::frame::Stats>>),
) {
    let dt = time.delta_secs();
    // Schwierigkeit: die Gegner-KI bekommt mehr oder weniger je Ladung
    let ai_income = cfg.map_or(1.0, |c| c.ai_income());
    for (e, mut h, t, team, moving) in &mut harvesters {
        let me = flat(t.translation);
        // Nächste eigene Abladestelle (HQ oder Raffinerie)
        let depot = depots
            .iter()
            .filter(|(_, dt, _)| *dt == team)
            .map(|(dt, _, b)| (flat(dt.translation), b.radius))
            .min_by(|a, b| a.0.distance(me).total_cmp(&b.0.distance(me)));
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
                let Some((dp, dr)) = depot else { continue };
                if me.distance(dp) < dr + 1.8 {
                    let load = if *team == ENEMY { (h.cargo as f32 * ai_income).round() as u32 } else { h.cargo };
                    stock.0[team.0 as usize][h.cargo_kind.idx()] += load;
                    if let Some(s) = stats.as_mut() {
                        s.gathered[team.0 as usize][h.cargo_kind.idx()] += load;
                    }
                    h.cargo = 0;
                    commands.entity(e).remove::<MoveTo>();
                    h.state = if h.node.is_some() { HarvestState::ToNode } else { HarvestState::Idle };
                } else {
                    commands.entity(e).insert(MoveTo(dp));
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
    mut alerts: Option<ResMut<Alerts>>,
) {
    let dt = time.delta_secs();
    for (e, mut t, mut p) in &mut q {
        let Some(tgt) = snap.0.get(&p.target) else {
            commands.entity(e).try_despawn();
            continue;
        };
        let aim = aim_point(tgt);
        let to = aim - p.base;
        let d = to.length();
        let step = p.speed * dt;
        if d <= step + tgt.radius * 0.4 {
            if tgt.team == PLAYER
                && let Some(a) = alerts.as_mut()
            {
                a.hit(tgt.pos);
            }
            if p.splash > 0.0 {
                // Flächenschaden: alle Gegner im Umkreis, nach außen schwächer
                for (oe, o) in &snap.0 {
                    if o.team == p.team {
                        continue;
                    }
                    let dist = (flat(o.pos) - flat(p.base)).length() - o.radius * 0.5;
                    if dist < p.splash
                        && let Ok(mut h) = health.get_mut(*oe)
                    {
                        h.hp -= p.damage * (1.0 - (dist.max(0.0) / p.splash) * 0.6);
                    }
                }
            } else if let Ok(mut h) = health.get_mut(p.target) {
                h.hp -= p.damage;
            }
            // Flächenschaden schlägt im Boden ein, sonst trifft das Geschoss das Ziel selbst
            let surface = if p.splash > 0.0 {
                Surface::Ground
            } else if tgt.is_building {
                Surface::Concrete
            } else {
                Surface::Armor
            };
            fx.write(FxEvent::Impact { pos: p.base, caliber: p.caliber, surface });
            commands.entity(e).try_despawn();
            continue;
        }
        let dir = to / d;
        p.base += dir * step;
        p.traveled += step;
        // Bogenschuss: Höhe über der geraden Bahn als Parabel
        let f = (p.traveled / p.total).clamp(0.0, 1.0);
        let lift = p.arc * p.total * 4.0 * f * (1.0 - f);
        let slope = p.arc * 4.0 * (1.0 - 2.0 * f);
        let prev = t.translation;
        t.translation = p.base + Vec3::Y * lift;
        let heading = (dir + Vec3::Y * slope).normalize_or_zero();
        if (t.translation - prev).length_squared() > 1e-6 {
            t.rotation = Quat::from_rotation_arc(Vec3::Y, heading);
        }
        if p.rocket || p.arc > 0.0 {
            p.smoke_timer -= dt;
            if p.smoke_timer <= 0.0 {
                p.smoke_timer = if p.rocket { 0.05 } else { 0.08 };
                fx.write(FxEvent::Trail(t.translation - heading * 0.6));
            }
        }
    }
}

pub fn deaths(
    mut commands: Commands,
    mut fx: MessageWriter<FxEvent>,
    q: Query<(Entity, &Health, &Transform, Option<&Building>, Option<&Unit>, Option<&Team>)>,
) {
    for (e, h, t, building, unit, team) in &q {
        if h.hp > 0.0 {
            continue;
        }
        let wreck = unit.zip(team).map(|(u, team)| crate::fx::WreckInfo { kind: u.kind, team: *team, rot: t.rotation });
        fx.write(FxEvent::Death { pos: t.translation, big: building.is_some_and(|b| b.radius > 2.5), wreck });
        match building {
            // Gebäude scheiden sofort aus dem Spiel aus, das Modell stürzt aber erst noch ein
            Some(b) => {
                commands
                    .entity(e)
                    .remove::<(Team, Health, Building, Structure, Hq, Depot, Weapon, AttackTarget, TurretRef, RisePivot, Construction, Selected)>()
                    .insert(crate::fx::Collapse::new(t, b.radius));
            }
            None => commands.entity(e).try_despawn(),
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

pub fn has_factory(q: &Query<(&Team, &Structure), Without<Construction>>, team: Team) -> bool {
    q.iter().any(|(t, s)| *t == team && s.0 == BuildingKind::Fabrik)
}

pub fn enqueue_builds(
    mut reqs: MessageReader<BuildRequest>,
    mut stock: ResMut<Stock>,
    mut hqs: Query<(&Team, &mut Hq)>,
    factories: Query<(&Team, &Structure), Without<Construction>>,
    unlocks: Option<Res<crate::campaign::Unlocks>>,
) {
    for r in reqs.read() {
        let mut kind = r.kind;
        // Kampagne: nur Freigeschaltetes. Die Gegner-KI baut ersatzweise Panzer.
        if let Some(u) = &unlocks
            && !u.unit(r.team, kind)
        {
            if r.team == PLAYER || !u.unit(r.team, UnitKind::Panzer) {
                continue;
            }
            kind = UnitKind::Panzer;
        }
        if kind.needs_factory() && !has_factory(&factories, r.team) {
            continue;
        }
        for (team, mut hq) in &mut hqs {
            if *team != r.team || hq.queue.len() >= MAX_QUEUE {
                continue;
            }
            let cost = kind.cost();
            if stock.can_afford(r.team, cost) {
                stock.pay(r.team, cost);
                hq.queue.push(kind);
            }
        }
    }
}

/// Jede fertige Fabrik beschleunigt die Produktion um 40 % (höchstens drei zählen)
pub fn production_rate(factories: usize) -> f32 {
    1.0 + 0.4 * factories.min(3) as f32
}

pub fn production(
    mut commands: Commands,
    time: Res<Time>,
    kit: Res<ModelKit>,
    mut hqs: Query<(&Team, &mut Hq, &Transform, Option<&Rally>)>,
    factories: Query<(&Team, &Structure), Without<Construction>>,
) {
    for (team, mut hq, t, rally) in &mut hqs {
        let n = factories.iter().filter(|(ft, s)| *ft == team && s.0 == BuildingKind::Fabrik).count();
        let Some(&kind) = hq.queue.first() else {
            hq.progress = 0.0;
            continue;
        };
        hq.progress += time.delta_secs() * production_rate(n);
        if hq.progress >= kind.build_time() {
            hq.progress = 0.0;
            hq.queue.remove(0);
            let dir = if *team == PLAYER { Vec3::new(1.0, 0.0, -1.0) } else { Vec3::new(-1.0, 0.0, 1.0) }.normalize();
            let spawn = flat(t.translation) + dir * 7.0;
            let e = spawn_unit(&mut commands, &kit, *team, kind, spawn);
            match (kind, rally) {
                // Sammler mit Sammelpunkt auf einem Feld bauen dort ab
                (UnitKind::Sammler, Some(Rally { node: Some(node), .. })) => {
                    commands.entity(e).insert(Harvester { state: HarvestState::ToNode, node: Some(*node), cargo: 0, cargo_kind: ResKind::Ferrit });
                }
                (UnitKind::Sammler, _) => {}
                (_, Some(r)) => {
                    commands.entity(e).insert(MoveTo(r.pos));
                }
                _ => {
                    commands.entity(e).insert(MoveTo(spawn + dir * 6.0));
                }
            }
        }
    }
}

/// Werkstatt: so nah am eigenen HQ werden Fahrzeuge repariert, solange sie nicht kämpfen
pub const REPAIR_RANGE: f32 = 16.0;
pub const REPAIR_RATE: f32 = 6.0;

pub fn repair(
    time: Res<Time>,
    hqs: Query<(&Transform, &Team), With<Hq>>,
    mut q: Query<(&Transform, &Team, &mut Health), (With<Unit>, Without<AttackTarget>)>,
) {
    for (t, team, mut h) in &mut q {
        if h.hp >= h.max {
            continue;
        }
        if hqs.iter().any(|(ht, hteam)| hteam == team && dist2d(ht.translation, t.translation) < REPAIR_RANGE) {
            h.hp = (h.hp + REPAIR_RATE * time.delta_secs()).min(h.max);
        }
    }
}

/// Fahrdaten fortschreiben: Strecke, Tempo, Rückstoß. Dazu Staubfahne und Fahrspuren.
pub fn vehicle_motion(
    time: Res<Time>,
    mut rng: ResMut<Rng>,
    mut q: Query<(&Transform, &Unit, &mut Motion)>,
    mut fx: MessageWriter<FxEvent>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (t, unit, mut m) in &mut q {
        let delta = flat(t.translation - m.prev);
        m.prev = t.translation;
        // Sprünge (Spawn, Wegschieben) nicht als Fahrt werten
        if delta.length() > 2.0 {
            continue;
        }
        let fwd = flat(t.rotation * Vec3::NEG_Z).normalize_or_zero();
        let signed = delta.dot(fwd);
        m.dist += signed;
        m.speed += (signed / dt - m.speed) * (6.0 * dt).min(1.0);
        m.recoil *= (-5.0 * dt).exp();
        if m.speed.abs() < 0.8 {
            continue;
        }
        let back = t.translation - fwd * 1.3;
        m.dust_t -= dt;
        if m.dust_t <= 0.0 {
            m.dust_t = 0.22 - (m.speed.abs() * 0.01).min(0.1);
            let side = Vec3::new(fwd.z, 0.0, -fwd.x) * unit.kind.gauge() * if rng.next_f32() < 0.5 { 1.0 } else { -1.0 };
            fx.write(FxEvent::Dust { pos: back + side + Vec3::Y * 0.2, size: 0.5 + m.speed.abs() * 0.08 });
        }
        m.track_d -= signed.abs();
        if m.track_d <= 0.0 {
            m.track_d = 1.4;
            let rot = Quat::from_rotation_y(f32::atan2(-fwd.x, -fwd.z));
            fx.write(FxEvent::Track { pos: t.translation, rot, width: unit.kind.gauge() * 2.6 });
        }
    }
}

/// Räder, Antriebs- und Leiträder drehen sich passend zur gefahrenen Strecke
pub fn spin_wheels(motion: Query<&Motion>, mut wheels: Query<(&Wheel, &ChildOf, &mut Transform)>) {
    for (w, parent, mut t) in &mut wheels {
        if let Ok(m) = motion.get(parent.parent()) {
            t.rotation = Quat::from_rotation_x(-m.dist / w.radius);
        }
    }
}

/// Ladung im Sammler, verrußter Lack bei schweren Schäden, Staub beim Abbauen
pub fn vehicle_visuals(
    time: Res<Time>,
    kit: Res<ModelKit>,
    mut timer: Local<f32>,
    harvesters: Query<(&Harvester, &CargoRef, &Transform)>,
    damaged: Query<(&Health, &PaintParts), Changed<Health>>,
    mut parts: Query<(&mut Transform, &mut Visibility), Without<Harvester>>,
    mut mats: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut fx: MessageWriter<FxEvent>,
) {
    *timer -= time.delta_secs();
    let puff = *timer <= 0.0;
    if puff {
        *timer = 0.3;
    }
    for (h, cargo, t) in &harvesters {
        let fill = match h.state {
            HarvestState::Mining(e) => {
                if puff {
                    fx.write(FxEvent::Dust { pos: t.translation + t.rotation * Vec3::new(0.6, 0.3, 1.8), size: 0.9 });
                }
                (e / HARVEST_TIME).clamp(0.0, 1.0)
            }
            _ => h.cargo as f32 / HARVEST_LOAD as f32,
        };
        if let Ok((mut ct, mut vis)) = parts.get_mut(cargo.0) {
            *vis = if fill > 0.02 { Visibility::Inherited } else { Visibility::Hidden };
            let xz = 0.75 + 0.25 * fill;
            ct.scale = Vec3::new(xz, fill.max(0.01), xz);
            ct.translation.y = 0.67 + 0.31 * fill;
        }
    }
    for (hp, paint) in &damaged {
        let mat = if hp.hp / hp.max < 0.4 { &kit.vehicle_mat_damaged } else { &kit.vehicle_mat };
        for e in &paint.0 {
            if let Ok(mut m) = mats.get_mut(*e)
                && m.0 != *mat
            {
                m.0 = mat.clone();
            }
        }
    }
}
