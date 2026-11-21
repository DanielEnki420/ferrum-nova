//! Baubare Gebäude: Platzieren mit Vorschau, Errichten, Funktion.
//!
//! - Der Spieler wählt ein Gebäude (Taste 7–9 oder Knopf). Eine durchscheinende Vorschau folgt
//!   der Maus, grün auf gültigem Platz, rot sonst. Linksklick baut, Shift hält den Baumodus,
//!   Rechtsklick oder Esc bricht ab.
//! - Gebaut wird nur in Reichweite eigener Gebäude, auf nicht zu steilem Boden und mit Abstand
//!   zu anderen Gebäuden und Rohstofffeldern. Die Kosten werden beim Platzieren bezahlt.
//! - Das Gebäude wächst dann aus dem Boden. Erst wenn es fertig ist, arbeitet es:
//!   Geschütztürme schießen, Raffinerien nehmen Ladungen an, Fabriken beschleunigen die Produktion.

use crate::components::*;
use crate::fx::FxEvent;
use crate::input::{ground_point, over_ui, MainCamera};
use crate::models::{self, ModelKit};
use crate::terrain;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::f32::consts::{PI, TAU};

/// So weit um eigene Gebäude darf gebaut werden (ab deren Rand)
pub const BUILD_RANGE: f32 = 24.0;

/// Bauauftrag (Spieler oder KI). Wird beim Ausführen noch einmal geprüft.
#[derive(Message)]
pub struct PlaceRequest {
    pub team: Team,
    pub kind: BuildingKind,
    pub pos: Vec3,
    /// Ausrichtung (Mauern und Tore), sonst Tor zur Kartenmitte
    pub rot: Option<Quat>,
}

/// Baumodus des Spielers
#[derive(Resource, Default)]
pub struct Placing {
    pub kind: Option<BuildingKind>,
    ghost: Option<(Entity, BuildingKind, bool)>,
    /// Dieser Mausklick gehörte dem Baumodus (nicht als Auswahl werten)
    pub block_click: bool,
    /// Mauer: Startpunkt der Linie
    line_start: Option<Vec3>,
    /// Mauer: Stücke und Gesamtkosten der aktuellen Linie (für die Hinweiszeile)
    pub line_info: Option<(usize, [u32; 2])>,
}

/// Blickrichtung neuer Gebäude: Tor zur Kartenmitte
pub fn facing(team: Team) -> Vec3 {
    if team == PLAYER { Vec3::new(1.0, 0.0, -1.0) } else { Vec3::new(-1.0, 0.0, 1.0) }.normalize()
}

/// Ein bestehendes Gebäude für die Platzprüfung
#[derive(Clone, Copy)]
pub struct Site {
    pub pos: Vec3,
    pub radius: f32,
    pub team: Team,
    pub wall: bool,
    pub gate: bool,
}

pub fn can_place(kind: BuildingKind, pos: Vec3, team: Team, sites: &[Site], nodes: &[Vec3]) -> bool {
    let r = kind.radius();
    let lim = MAP_HALF - 8.0;
    if pos.x.abs() > lim || pos.z.abs() > lim {
        return false;
    }
    // Bauen nur nahe eigener Gebäude (Mauern verlängern die Reichweite nicht)
    let near_base = sites
        .iter()
        .any(|s| s.team == team && !s.wall && flat(s.pos).distance(flat(pos)) < BUILD_RANGE + s.radius);
    if !near_base {
        return false;
    }
    let too_close = sites.iter().any(|s| {
        let own = s.team == team;
        let min = match (kind, s.wall, s.gate) {
            // Mauerstücke reihen sich aneinander, auch über Ecken
            (BuildingKind::Mauer, true, false) if own => 1.3,
            (BuildingKind::Mauer, true, true) if own => r + s.radius - 0.4,
            // ein Tor ersetzt eigene Mauerstücke
            (BuildingKind::Tor, true, false) if own => return false,
            _ if kind.is_wall() || s.wall => r + s.radius + 0.4,
            _ => r + s.radius + 1.5,
        };
        flat(s.pos).distance(flat(pos)) < min
    });
    if too_close {
        return false;
    }
    let node_gap = if kind.is_wall() { 3.0 } else { 4.0 };
    if nodes.iter().any(|n| flat(*n).distance(flat(pos)) < r + node_gap) {
        return false;
    }
    // Boden darf nicht zu uneben sein
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for i in 0..9 {
        let (a, d) = (TAU * i as f32 / 8.0, if i == 8 { 0.0 } else { r });
        let h = terrain::height(pos.x + a.cos() * d, pos.z + a.sin() * d);
        lo = lo.min(h);
        hi = hi.max(h);
    }
    hi - lo < 1.3
}

/// Freien Platz nahe `target` suchen (für die KI)
pub fn find_spot(kind: BuildingKind, target: Vec3, team: Team, sites: &[Site], nodes: &[Vec3]) -> Option<Vec3> {
    for ring in 0..8 {
        let d = ring as f32 * 2.0;
        let n = if ring == 0 { 1 } else { 6 + ring * 3 };
        for i in 0..n {
            let a = TAU * i as f32 / n as f32 + ring as f32 * 0.7;
            let p = target + Vec3::new(a.cos() * d, 0.0, a.sin() * d);
            if can_place(kind, p, team, sites, nodes) {
                return Some(p);
            }
        }
    }
    None
}

/// Neues Gebäude im Bau anlegen
pub fn spawn_structure(c: &mut Commands, kit: &ModelKit, team: Team, kind: BuildingKind, pos: Vec3, rot: Option<Quat>) -> Entity {
    structure(c, kit, team, kind, pos, rot, false)
}

/// Fertiges Gebäude ohne Bauphase (vorgebaute Stellungen in Missionen)
pub fn spawn_finished_structure(c: &mut Commands, kit: &ModelKit, team: Team, kind: BuildingKind, pos: Vec3, rot: Option<Quat>) -> Entity {
    structure(c, kit, team, kind, pos, rot, true)
}

fn structure(c: &mut Commands, kit: &ModelKit, team: Team, kind: BuildingKind, pos: Vec3, rot: Option<Quat>, finished: bool) -> Entity {
    let ground = terrain::on_ground(pos);
    let depth = kit.structures[kind.idx()][team.0 as usize].height;
    let tf = Transform::from_translation(ground).looking_to(-facing(team), Vec3::Y);
    let root = c
        .spawn((
            rot.map_or(tf, |r| tf.with_rotation(r)),
            Visibility::default(),
            team,
            Health { hp: if finished { kind.hp() } else { kind.hp() * 0.1 }, max: kind.hp() },
            Building { radius: kind.radius() },
            Structure(kind),
        ))
        .id();
    if !finished {
        c.entity(root).insert(Construction { progress: 0.0, dust: 0.0 });
    }
    let pivot = c.spawn((Transform::from_xyz(0.0, if finished { 0.0 } else { -depth }, 0.0), Visibility::default())).id();
    c.entity(root).add_child(pivot).insert(RisePivot { entity: pivot, depth });
    if let Some(head) = models::build_structure(c, kit, pivot, team, kind, None) {
        c.entity(root).insert(TurretRef(head));
    }
    match kind {
        BuildingKind::Geschuetzturm => {
            c.entity(root).insert(Weapon { range: 17.0, damage: 24.0, cooldown: 1.2, timer: 0.0, projectile_speed: 44.0, rocket: false, splash: 0.0, arc: 0.0 });
        }
        BuildingKind::Raffinerie => {
            c.entity(root).insert(Depot);
        }
        BuildingKind::Fabrik => {}
        BuildingKind::Raketenturm => {
            c.entity(root).insert(Weapon { range: 24.0, damage: 30.0, cooldown: 2.2, timer: 0.0, projectile_speed: 22.0, rocket: true, splash: 2.5, arc: 0.0 });
        }
        BuildingKind::Mauer => {
            c.entity(root).insert(Wall);
        }
        BuildingKind::Tor => {
            c.entity(root).insert((Wall, Gate));
        }
    }
    root
}

type SiteQuery<'w, 's> = Query<'w, 's, (&'static Transform, &'static Building, &'static Team, Has<Wall>, Has<Gate>)>;

fn collect_sites(q: &SiteQuery) -> Vec<Site> {
    q.iter().map(|(t, b, team, wall, gate)| Site { pos: t.translation, radius: b.radius, team: *team, wall, gate }).collect()
}

/// Mittelpunkte der 2-m-Mauerstücke von `a` Richtung `b` und das tatsächliche Linienende
pub fn wall_segments(a: Vec3, b: Vec3) -> (Vec<Vec3>, Vec3) {
    let d = flat(b - a);
    let len = d.length();
    if len < 0.5 {
        return (vec![a], a);
    }
    let dir = d / len;
    let n = ((len / 2.0).round() as usize).max(1);
    let centers = (0..n).map(|i| a + dir * (1.0 + 2.0 * i as f32)).collect();
    (centers, a + dir * (2.0 * n as f32))
}

/// Ausrichtung eines Mauerstücks entlang `dir` (lokale X-Achse)
pub fn along(dir: Vec3) -> Quat {
    let d = flat(dir).normalize_or(Vec3::X);
    Quat::from_rotation_y((-d.z).atan2(d.x))
}

/// Baumodus des Spielers: Tasten, Vorschau, Klick
#[allow(clippy::too_many_arguments)]
pub fn placement_input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    ui_scale: Res<UiScale>,
    kit: Res<ModelKit>,
    stock: Res<Stock>,
    mut placing: ResMut<Placing>,
    mut outbox: ResMut<crate::net::cmd::Outbox>,
    sites: SiteQuery,
    walls: Query<(&Transform, &Team, &Structure), With<Building>>,
    nodes: Query<&Transform, With<ResourceNode>>,
    mut ghosts: Query<(&mut Transform, &mut Visibility), (Without<Building>, Without<ResourceNode>)>,
    mut gizmos: Gizmos,
) {
    let me = crate::net::me();
    for (key, kind) in [
        (KeyCode::KeyG, BuildingKind::Geschuetzturm),
        (KeyCode::KeyH, BuildingKind::Raffinerie),
        (KeyCode::KeyJ, BuildingKind::Fabrik),
        (KeyCode::KeyK, BuildingKind::Mauer),
        (KeyCode::KeyI, BuildingKind::Tor),
        (KeyCode::KeyU, BuildingKind::Raketenturm),
    ] {
        if keys.just_pressed(key) {
            placing.kind = if placing.kind == Some(kind) { None } else { Some(kind) };
            placing.line_start = None;
        }
    }
    if placing.kind.is_some() && (keys.just_pressed(KeyCode::Escape) || mouse.just_pressed(MouseButton::Right)) {
        // Mauer: erst die angefangene Linie verwerfen, dann den Baumodus
        if placing.line_start.is_some() {
            placing.line_start = None;
        } else {
            placing.kind = None;
        }
        placing.block_click = true;
    }
    placing.line_info = None;

    let Some(kind) = placing.kind else {
        placing.line_start = None;
        if let Some((g, _, _)) = placing.ghost.take() {
            commands.entity(g).try_despawn();
        }
        return;
    };

    let (Ok(win), Ok((camera, cam_t))) = (window.single(), cam.single()) else { return };
    let point = win
        .cursor_position()
        .filter(|c| !over_ui(win, *c, ui_scale.0))
        .and_then(|c| ground_point(camera, cam_t, c));

    let site_list = collect_sites(&sites);
    let node_list: Vec<Vec3> = nodes.iter().map(|t| t.translation).collect();
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);

    // Bauradius um eigene Gebäude
    for s in site_list.iter().filter(|s| s.team == me && !s.wall) {
        let c = terrain::on_ground(s.pos) + Vec3::Y * 0.3;
        gizmos.circle(Isometry3d::new(c, Quat::from_rotation_x(PI / 2.0)), BUILD_RANGE + s.radius, Color::srgba(0.5, 0.8, 1.0, 0.25));
    }

    // --- Mauer: Linie von Klick zu Klick, Vorschau als Umrisse
    if kind == BuildingKind::Mauer {
        if let Some((g, _, _)) = placing.ghost.take() {
            commands.entity(g).try_despawn();
        }
        let Some(p) = point else { return };
        let start = placing.line_start;
        let (segs, end) = match start {
            Some(a) => wall_segments(a, p),
            None => (vec![p], p),
        };
        let rot = along(start.map_or(Vec3::X, |a| p - a));
        // Gültigkeit Stück für Stück, Kosten laufen mit
        let mut sim = site_list.clone();
        let mut budget = stock.0[me.0 as usize];
        let cost = kind.cost();
        let mut ok = Vec::new();
        for c in &segs {
            let afford = budget[0] >= cost[0] && budget[1] >= cost[1];
            let valid = afford && can_place(kind, *c, me, &sim, &node_list);
            if valid {
                budget[0] -= cost[0];
                budget[1] -= cost[1];
                sim.push(Site { pos: *c, radius: kind.radius(), team: me, wall: true, gate: false });
            }
            ok.push(valid);
            let col = if valid { Color::srgb(0.4, 1.0, 0.5) } else { Color::srgb(1.0, 0.35, 0.3) };
            let base = terrain::on_ground(*c);
            for h in [0.15, 1.9] {
                gizmos.rect(Isometry3d::new(base + Vec3::Y * h, rot * Quat::from_rotation_x(PI / 2.0)), Vec2::new(2.0, 0.8), col);
            }
        }
        let n_ok = ok.iter().filter(|v| **v).count();
        placing.line_info = Some((n_ok, [cost[0] * n_ok as u32, cost[1] * n_ok as u32]));
        if mouse.just_pressed(MouseButton::Left) {
            placing.block_click = true;
            match start {
                None => placing.line_start = Some(p),
                Some(_) => {
                    for (c, valid) in segs.iter().zip(&ok) {
                        if *valid {
                            outbox.push(crate::net::cmd::Cmd::Place { kind, pos: *c, rot: Some(rot) });
                        }
                    }
                    // Shift: am Ende weiterziehen, sonst fertig
                    if shift {
                        placing.line_start = Some(end);
                    } else {
                        placing.line_start = None;
                        placing.kind = None;
                    }
                }
            }
        }
        return;
    }

    // Tor: am nächsten eigenen Mauerstück einrasten und dessen Richtung übernehmen
    let (point, rot) = match (kind, point) {
        (BuildingKind::Tor, Some(p)) => {
            let snap = walls
                .iter()
                .filter(|(t, team, s)| **team == me && s.0 == BuildingKind::Mauer && flat(t.translation).distance(flat(p)) < 3.0)
                .min_by(|a, b| flat(a.0.translation).distance(flat(p)).total_cmp(&flat(b.0.translation).distance(flat(p))));
            match snap {
                Some((t, _, _)) => (Some(t.translation), Some(t.rotation)),
                None => (Some(p), None),
            }
        }
        _ => (point, None),
    };

    let valid = point.is_some_and(|p| can_place(kind, p, me, &site_list, &node_list) && stock.can_afford(me, kind.cost()));

    // Vorschau neu aufbauen, wenn sich Typ oder Gültigkeit ändert
    let default_rot = Transform::default().looking_to(-facing(me), Vec3::Y).rotation;
    if placing.ghost.is_none_or(|(_, k, v)| k != kind || v != valid) {
        if let Some((g, _, _)) = placing.ghost.take() {
            commands.entity(g).try_despawn();
        }
        let root = commands
            .spawn((
                Transform::from_translation(terrain::on_ground(point.unwrap_or_default())).with_rotation(rot.unwrap_or(default_rot)),
                if point.is_some() { Visibility::Visible } else { Visibility::Hidden },
            ))
            .id();
        let mat = &kit.ghost[if valid { 0 } else { 1 }];
        models::build_structure(&mut commands, &kit, root, me, kind, Some(mat));
        placing.ghost = Some((root, kind, valid));
    } else if let Some((g, _, _)) = placing.ghost
        && let Ok((mut t, mut vis)) = ghosts.get_mut(g)
    {
        match point {
            Some(p) => {
                t.translation = terrain::on_ground(p);
                t.rotation = rot.unwrap_or(default_rot);
                *vis = Visibility::Visible;
            }
            None => *vis = Visibility::Hidden,
        }
    }

    let Some(p) = point else { return };
    let col = if valid { Color::srgb(0.4, 1.0, 0.5) } else { Color::srgb(1.0, 0.35, 0.3) };
    let ground = terrain::on_ground(p) + Vec3::Y * 0.2;
    gizmos.circle(Isometry3d::new(ground, Quat::from_rotation_x(PI / 2.0)), kind.radius(), col);

    if mouse.just_pressed(MouseButton::Left) {
        placing.block_click = true;
        if valid {
            outbox.push(crate::net::cmd::Cmd::Place { kind, pos: p, rot });
            if !shift {
                placing.kind = None;
            }
        }
    }
}

/// Bauaufträge prüfen, bezahlen und ausführen
pub fn place_buildings(
    mut commands: Commands,
    mut reqs: MessageReader<PlaceRequest>,
    mut stock: ResMut<Stock>,
    kit: Res<ModelKit>,
    sites: SiteQuery,
    walls: Query<(Entity, &Transform, &Team, &Structure)>,
    nodes: Query<&Transform, With<ResourceNode>>,
    mut fx: MessageWriter<FxEvent>,
    unlocks: Option<Res<crate::campaign::Unlocks>>,
) {
    let mut site_list = collect_sites(&sites);
    let node_list: Vec<Vec3> = nodes.iter().map(|t| t.translation).collect();
    for r in reqs.read() {
        if unlocks.as_ref().is_some_and(|u| !u.building(r.team, r.kind)) {
            continue;
        }
        if !stock.can_afford(r.team, r.kind.cost()) || !can_place(r.kind, r.pos, r.team, &site_list, &node_list) {
            continue;
        }
        stock.pay(r.team, r.kind.cost());
        // Ein Tor ersetzt die eigenen Mauerstücke an seiner Stelle
        if r.kind == BuildingKind::Tor {
            for (e, t, team, s) in &walls {
                if *team == r.team && s.0 == BuildingKind::Mauer && flat(t.translation).distance(flat(r.pos)) < 3.6 {
                    commands.entity(e).try_despawn();
                    site_list.retain(|site| flat(site.pos).distance(flat(t.translation)) > 0.01);
                }
            }
        }
        spawn_structure(&mut commands, &kit, r.team, r.kind, r.pos, r.rot);
        site_list.push(Site { pos: r.pos, radius: r.kind.radius(), team: r.team, wall: r.kind.is_wall(), gate: r.kind == BuildingKind::Tor });
        fx.write(FxEvent::BuildDust { pos: terrain::on_ground(r.pos), radius: r.kind.radius() });
    }
}

/// Gebäude wachsen aus dem Boden; Staub steigt auf, die Lebenspunkte füllen sich
pub fn construct(
    mut commands: Commands,
    time: Res<Time>,
    mut rng: ResMut<Rng>,
    mut q: Query<(Entity, &mut Construction, &mut Health, &Structure, &RisePivot, &Transform)>,
    mut pivots: Query<&mut Transform, Without<Construction>>,
    mut fx: MessageWriter<FxEvent>,
) {
    let dt = time.delta_secs();
    for (e, mut c, mut hp, s, rise, t) in &mut q {
        let step = dt / s.0.build_time();
        c.progress = (c.progress + step).min(1.0);
        hp.hp = (hp.hp + hp.max * 0.9 * step).min(hp.max);
        let k = c.progress;
        let ease = 1.0 - (1.0 - k) * (1.0 - k);
        if let Ok(mut pt) = pivots.get_mut(rise.entity) {
            pt.translation.y = -rise.depth * (1.0 - ease);
        }
        c.dust -= dt;
        if c.dust <= 0.0 {
            c.dust = 0.18;
            let r = s.0.radius();
            let a = rng.range(0.0, TAU);
            let off = Vec3::new(a.cos() * r * 0.9, 0.3, a.sin() * r * 0.9);
            fx.write(FxEvent::Puff { pos: t.translation + off, dark: false });
        }
        if c.progress >= 1.0 {
            commands.entity(e).remove::<Construction>();
            fx.write(FxEvent::BuildDust { pos: t.translation, radius: s.0.radius() });
        }
    }
}

/// Geschützköpfe drehen sich zum Ziel, sonst schwenken sie langsam
pub fn structure_aim(
    time: Res<Time>,
    snap: Res<crate::units::Snapshot>,
    roots: Query<(&Transform, &TurretRef, Option<&AttackTarget>), (With<Structure>, Without<Construction>)>,
    mut heads: Query<&mut Transform, (With<GunHead>, Without<Structure>)>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    for (t, tref, atk) in &roots {
        let Ok(mut ht) = heads.get_mut(tref.0) else { continue };
        let (root_yaw, _, _) = t.rotation.to_euler(EulerRot::YXZ);
        let want = match atk.and_then(|a| snap.get(&a.0)) {
            Some(tgt) => {
                let to = flat(tgt.pos - t.translation);
                Heading::from_dir(to).0 - root_yaw
            }
            // Wache: langsam hin und her schwenken
            None => (now * 0.25 + t.translation.x).sin() * 0.9,
        };
        let (cur, _, _) = ht.rotation.to_euler(EulerRot::YXZ);
        let diff = (want - cur + PI).rem_euclid(TAU) - PI;
        let speed = if atk.is_some() { 2.2 } else { 0.4 };
        ht.rotation = Quat::from_rotation_y(cur + diff.clamp(-speed * dt, speed * dt));
    }
}

/// Tore öffnen sich, wenn eigene Einheiten nahe sind: der Flügel fährt in den Boden
pub fn gate_doors(
    time: Res<Time>,
    mut doors: Query<(&mut models::GateDoor, &mut Transform, &ChildOf)>,
    pivots: Query<&ChildOf, Without<models::GateDoor>>,
    roots: Query<(&Transform, &Team), (With<Gate>, Without<models::GateDoor>, Without<Construction>)>,
    units: Query<(&Transform, &Team), (With<Unit>, Without<models::GateDoor>)>,
) {
    let dt = time.delta_secs();
    for (mut door, mut t, parent) in &mut doors {
        let Ok(root) = pivots.get(parent.parent()) else { continue };
        let Ok((rt, team)) = roots.get(root.parent()) else { continue };
        let near = units.iter().any(|(ut, ut_team)| ut_team == team && flat(ut.translation).distance(flat(rt.translation)) < 7.5);
        let target = if near { 1.0 } else { 0.0 };
        let speed = if near { 1.6 } else { 0.8 };
        door.open += (target - door.open).clamp(-speed * dt, speed * dt);
        t.translation.y = -2.1 * door.open;
    }
}
