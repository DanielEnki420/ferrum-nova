//! Fester Simulationstakt: Die Spiellogik läuft in Schritten von genau 1/20 s (`SimTick`), egal
//! wie schnell der Rechner Bilder zeichnet. Im Duell rechnen so beide Browser exakt dasselbe,
//! solange sie dieselben Befehle zum selben Tick bekommen (net.rs). Im Einzelspieler läuft
//! derselbe Takt, nur ohne Netz.
//!
//! Dazu gehört:
//! - `NetId`: stabile Nummern für Sim-Entities, in fester Reihenfolge vergeben
//! - `SimSight`: was jedes Team gerade sieht (gleich auf beiden Rechnern, für die Zielwahl)
//! - `SimHash`: Prüfsumme des Spielstands, um Abweichungen (Desync) zu erkennen
//! - Interpolation: Zwischen zwei Ticks werden Einheiten und Geschosse weich dargestellt

use crate::components::*;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::ecs::schedule::SingleThreadedExecutor;
use bevy::prelude::*;
use std::time::Duration;

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SimTick;

/// Ticks je Sekunde
pub const HZ: u32 = 20;
pub const DT: f32 = 1.0 / HZ as f32;
const DT_DURATION: Duration = Duration::from_millis(1000 / HZ as u64);
/// So viele Ticks holt ein Bild höchstens nach (danach wird die Zeit verworfen)
const MAX_PER_FRAME: u32 = 6;
/// Alle so viele Ticks eine Prüfsumme
pub const HASH_EVERY: u32 = 20;

/// Spielregeln, die von der Art der Partie abhängen (gleich auf beiden Rechnern)
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SimRules {
    /// Duell zwischen zwei Menschen: keine Gegner-KI, beide Teams mit Nebel und gleichen Einnahmen
    pub duel: bool,
}

#[derive(Resource)]
pub struct SimClock {
    /// Nummer des nächsten Ticks
    pub tick: u32,
    acc: f32,
    time: Time<()>,
    /// Anteil des laufenden Ticks (0..1) für die Darstellung
    pub alpha: f32,
}

impl Default for SimClock {
    fn default() -> Self {
        Self { tick: 0, acc: 0.0, time: Time::default(), alpha: 1.0 }
    }
}

/// Zähler für neue `NetId`s
#[derive(Resource, Default)]
pub struct NetIds {
    next: u32,
}

/// Schedule für einen Tick, ohne Parallelität (feste Reihenfolge der Systeme)
pub fn schedule() -> Schedule {
    let mut s = Schedule::new(SimTick);
    s.set_executor(SingleThreadedExecutor::new());
    s
}

/// Einzelspieler (Gegner-KI und VAAR aktiv)?
pub fn solo(rules: Option<Res<SimRules>>) -> bool {
    !rules.is_some_and(|r| r.duel)
}

/// Takt, Ressourcen und alle Systeme der Spiellogik anmelden (Spiel und Tests)
pub fn add_sim(app: &mut App) {
    use crate::{ai, campaign, construction, frame, halkyon, nav, net, orders, units};
    app.add_schedule(schedule())
        .init_resource::<SimClock>()
        .init_resource::<NetIds>()
        .init_resource::<SimSight>()
        .init_resource::<SimHash>()
        .init_resource::<net::cmd::Outbox>()
        .init_resource::<net::cmd::TickCmds>()
        .add_message::<construction::RebuildRequest>();
    if !app.world().contains_resource::<SimRules>() {
        app.init_resource::<SimRules>();
    }
    app.add_systems(
        SimTick,
        (
            (
                net::cmd::apply_cmds,
                assign_ids,
                interp_begin,
                halkyon::tick_clock,
                update_sight,
                (ai::enemy_ai, ai::ai_tactics).chain().run_if(solo),
                units::enqueue_builds,
                construction::place_buildings,
                construction::rebuild,
                construction::construct,
                units::snapshot,
                nav::update_grid,
                units::auto_target,
                units::combat,
                assign_ids,
                orders::follow_orders,
                nav::plan_paths,
                units::movement,
                orders::face_on_arrival,
            )
                .chain(),
            (
                units::separation,
                units::vehicle_motion,
                units::ground_snap,
                units::turret_aim,
                construction::structure_aim,
                units::harvest,
                units::projectiles,
                units::production,
                units::repair,
                frame::track_stats,
                units::deaths,
                units::check_outcome,
                campaign::mission_tick.run_if(solo),
                orders::tick_alerts,
                record_hash,
            )
                .chain(),
        )
            .chain(),
    );
}

/// Einen Tick rechnen: `Time` zeigt dabei genau einen festen Schritt
pub fn run_tick(world: &mut World) {
    let fixed = {
        let mut clock = world.resource_mut::<SimClock>();
        clock.time.advance_by(DT_DURATION);
        clock.time.as_generic()
    };
    let saved = std::mem::replace(&mut *world.resource_mut::<Time>(), fixed);
    world.run_schedule(SimTick);
    *world.resource_mut::<Time>() = saved;
    world.resource_mut::<SimClock>().tick += 1;
}

/// Treiber im `Update`: so viele Ticks rechnen, wie Echtzeit vergangen ist und das Netz erlaubt
pub fn drive(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs().min(0.25);
    world.resource_mut::<SimClock>().acc += dt;
    let mut ran = 0;
    loop {
        let acc = world.resource::<SimClock>().acc;
        // im Duell holt der Rechner, der zurückliegt, schneller auf
        let behind = crate::net::ticks_behind(world);
        let need = acc >= DT || (behind > 2 && ran < 2);
        if !need || ran >= MAX_PER_FRAME {
            break;
        }
        if !crate::net::prepare_tick(world) {
            // warten auf den Mitspieler: Zeit nicht unbegrenzt aufstauen
            let mut c = world.resource_mut::<SimClock>();
            c.acc = c.acc.min(DT);
            break;
        }
        run_tick(world);
        crate::net::after_tick(world);
        ran += 1;
        {
            let mut c = world.resource_mut::<SimClock>();
            c.acc = (c.acc - DT).max(0.0);
        }
        if *world.resource::<Outcome>() != Outcome::Running {
            break;
        }
    }
    let mut c = world.resource_mut::<SimClock>();
    if ran >= MAX_PER_FRAME {
        c.acc = c.acc.min(DT);
    }
    c.alpha = (c.acc / DT).clamp(0.0, 1.0);
}

// ------------------------------------------------------------------ NetId

/// Ordnet Gleitkommazahlen so in u32, dass die Reihenfolge erhalten bleibt
fn ord(f: f32) -> u32 {
    let b = f.to_bits();
    if b & 0x8000_0000 != 0 { !b } else { b | 0x8000_0000 }
}

/// Neue Sim-Entities bekommen eine `NetId`. Die Reihenfolge hängt nur vom Inhalt ab (Art, Team,
/// Ort, Ziel), nicht davon, in welcher Reihenfolge Bevy sie gespeichert hat.
pub fn assign_ids(
    mut commands: Commands,
    mut ids: ResMut<NetIds>,
    fresh: Query<
        (Entity, &Transform, Option<&Team>, Option<&Unit>, Option<&Structure>, Option<&Projectile>, Option<&ResourceNode>, Has<Hq>),
        (Without<NetId>, Or<(With<Health>, With<Projectile>, With<ResourceNode>)>),
    >,
    known: Query<&NetId>,
) {
    if fresh.is_empty() {
        return;
    }
    let mut list: Vec<([u32; 7], Entity)> = fresh
        .iter()
        .map(|(e, t, team, unit, structure, shot, node, hq)| {
            let (class, kind) = match (unit, structure, shot, node) {
                (Some(u), ..) => (1, u.kind as u32),
                (_, Some(s), ..) => (2, s.0 as u32),
                (_, _, Some(p), _) => (4, p.caliber as u32),
                (_, _, _, Some(n)) => (5, n.kind.idx() as u32),
                _ => (if hq { 0 } else { 3 }, 0),
            };
            let target = shot.and_then(|p| known.get(p.target).ok()).map_or(u32::MAX, |i| i.0);
            let p = t.translation;
            ([class, team.map_or(9, |t| t.0 as u32), kind, ord(p.x), ord(p.z), ord(p.y), target], e)
        })
        .collect();
    list.sort_by_key(|(k, _)| *k);
    #[cfg(debug_assertions)]
    for w in list.windows(2) {
        if w[0].0 == w[1].0 {
            warn!("NetId: zwei neue Entities mit gleichem Schlüssel {:?}", w[0].0);
        }
    }
    for (_, e) in list {
        commands.entity(e).insert(NetId(ids.next));
        ids.next += 1;
    }
}

/// Entities in NetId-Reihenfolge (für Systeme, deren Ergebnis von der Reihenfolge abhängt)
pub fn in_order<'a>(it: impl Iterator<Item = (Entity, Option<&'a NetId>)>) -> Vec<Entity> {
    let mut v: Vec<(u64, Entity)> = it.map(|(e, id)| (id.map_or(u64::MAX / 2 + e.to_bits() / 2, |i| i.0 as u64), e)).collect();
    v.sort_by_key(|(k, _)| *k);
    v.into_iter().map(|(_, e)| e).collect()
}

// ------------------------------------------------------------------ Sicht

/// Sicht beider Teams, alle paar Ticks neu berechnet. Gilt für die Zielwahl der Menschen-Teams
/// (Einzelspieler: nur der Spieler, Duell: beide) und als Grundlage des Nebels.
#[derive(Resource)]
pub struct SimSight {
    pub visible: [Vec<bool>; 2],
    /// Wurde schon einmal gerechnet?
    pub ready: bool,
}

impl Default for SimSight {
    fn default() -> Self {
        let n = crate::fog::N * crate::fog::N;
        Self { visible: [vec![false; n], vec![false; n]], ready: false }
    }
}

impl SimSight {
    pub fn sees(&self, team: Team, p: Vec3) -> bool {
        !self.ready || crate::fog::cell(p).is_none_or(|i| self.visible[team.0 as usize & 1][i])
    }
}

/// Gilt die Nebelregel bei der Zielwahl für dieses Team?
pub fn fogged_team(rules: Option<&SimRules>, team: Team) -> bool {
    team == PLAYER || rules.is_some_and(|r| r.duel)
}

pub fn update_sight(
    clock: Res<SimClock>,
    mut sight: ResMut<SimSight>,
    units: Query<(&Transform, &Team, &Unit)>,
    buildings: Query<(&Transform, &Team, &Building, Option<&Weapon>, Has<Wall>, Has<Construction>)>,
    eclipse: Option<Res<crate::halkyon::Eclipse>>,
) {
    if sight.ready && !clock.tick.is_multiple_of(3) {
        return;
    }
    let dark = eclipse.map_or(0.0, |e| e.shade);
    let unit_k = 1.0 - crate::halkyon::UNIT_SIGHT_LOSS * dark;
    let building_k = 1.0 - crate::halkyon::BUILDING_SIGHT_LOSS * dark;
    for v in &mut sight.visible {
        v.iter_mut().for_each(|c| *c = false);
    }
    for (t, team, u) in &units {
        let v = &mut sight.visible[team.0 as usize & 1];
        crate::fog::circle(t.translation, crate::fog::unit_sight(u.kind) * unit_k, |i| v[i] = true);
    }
    for (t, team, b, w, wall, site) in &buildings {
        let v = &mut sight.visible[team.0 as usize & 1];
        crate::fog::circle(t.translation, crate::fog::building_sight(b, w, wall, site) * building_k, |i| v[i] = true);
    }
    sight.ready = true;
}

// ------------------------------------------------------------------ Prüfsumme

/// Prüfsummen der letzten Prüfpunkte (Tick, Wert)
#[derive(Resource, Default)]
pub struct SimHash {
    pub latest: Option<(u32, u64)>,
}

pub struct Fnv(u64);

impl Fnv {
    pub fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    pub fn u32(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    /// Gleitkommazahl als Bitmuster (NaN einheitlich)
    pub fn f32(&mut self, v: f32) {
        self.u32(if v.is_nan() { 0x7fc0_0000 } else { v.to_bits() });
    }
    pub fn vec3(&mut self, v: Vec3) {
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
    }
    pub fn finish(&self) -> u64 {
        self.0
    }
}

impl Default for Fnv {
    fn default() -> Self {
        Self::new()
    }
}

/// Prüfsumme über alles, was das Spiel entscheidet: Positionen, Lebenspunkte, Vorräte, Bauschlangen
pub fn state_hash(world: &mut World) -> u64 {
    let mut q = world.query::<(&NetId, &Transform, Option<&Health>, Option<&Team>, Option<&Hq>, Option<&Construction>, Option<&Harvester>)>();
    let mut rows: Vec<_> = q.iter(world).collect();
    rows.sort_by_key(|r| r.0);
    let mut h = Fnv::new();
    for (id, t, hp, team, hq, site, harvester) in rows {
        h.u32(id.0);
        h.vec3(t.translation);
        if let Some(hp) = hp {
            h.f32(hp.hp);
        }
        h.u32(team.map_or(9, |t| t.0 as u32));
        if let Some(hq) = hq {
            h.u32(hq.queue.len() as u32);
            h.f32(hq.progress);
        }
        if let Some(c) = site {
            h.f32(c.progress);
        }
        if let Some(hv) = harvester {
            h.u32(hv.cargo);
        }
    }
    if let Some(stock) = world.get_resource::<Stock>() {
        for team in stock.0 {
            h.u32(team[0]);
            h.u32(team[1]);
        }
    }
    h.finish()
}

/// Am Ende jedes Prüf-Ticks die Prüfsumme festhalten
pub fn record_hash(world: &mut World) {
    let tick = world.resource::<SimClock>().tick;
    if !tick.is_multiple_of(HASH_EVERY) {
        return;
    }
    let h = state_hash(world);
    world.resource_mut::<SimHash>().latest = Some((tick, h));
}

// ------------------------------------------------------------------ Interpolation

/// Lage vor dem letzten Tick und (während des Zeichnens) die echte Sim-Lage
#[derive(Component)]
pub struct SimXf {
    prev: Vec3,
    prev_rot: Quat,
    saved: Option<(Vec3, Quat)>,
}

/// Am Anfang eines Ticks: bisherige Lage merken (neue Einheiten bekommen die Komponente)
pub fn interp_begin(mut commands: Commands, mut q: Query<(Entity, &Transform, Option<&mut SimXf>), Or<(With<Unit>, With<Projectile>)>>) {
    for (e, t, xf) in &mut q {
        match xf {
            Some(mut xf) => {
                xf.prev = t.translation;
                xf.prev_rot = t.rotation;
            }
            None => {
                commands.entity(e).insert(SimXf { prev: t.translation, prev_rot: t.rotation, saved: None });
            }
        }
    }
}

/// Vor dem Zeichnen: zwischen vorheriger und aktueller Lage überblenden
pub fn interp_apply(clock: Option<Res<SimClock>>, mut q: Query<(&mut Transform, &mut SimXf)>) {
    let Some(clock) = clock else { return };
    let a = clock.alpha;
    for (mut t, mut xf) in &mut q {
        let cur = (t.translation, t.rotation);
        // große Sprünge (Spawn, Wegschieben aus Gebäuden) nicht überblenden
        if cur.0.distance_squared(xf.prev) > 25.0 {
            continue;
        }
        xf.saved = Some(cur);
        t.translation = xf.prev.lerp(cur.0, a);
        t.rotation = xf.prev_rot.slerp(cur.1, a);
    }
}

/// Zu Beginn des nächsten Bilds: die echte Sim-Lage wiederherstellen
pub fn interp_restore(mut q: Query<(&mut Transform, &mut SimXf)>) {
    for (mut t, mut xf) in &mut q {
        if let Some((p, r)) = xf.saved.take() {
            t.translation = p;
            t.rotation = r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_order_key_keeps_order() {
        let vals = [-50.5f32, -1.0, -0.0, 0.0, 0.25, 3.0, 70.0];
        for w in vals.windows(2) {
            assert!(ord(w[0]) <= ord(w[1]), "{} vs {}", w[0], w[1]);
        }
    }
}
