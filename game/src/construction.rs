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
}

/// Baumodus des Spielers
#[derive(Resource, Default)]
pub struct Placing {
    pub kind: Option<BuildingKind>,
    ghost: Option<(Entity, BuildingKind, bool)>,
    /// Dieser Mausklick gehörte dem Baumodus (nicht als Auswahl werten)
    pub block_click: bool,
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
}

pub fn can_place(kind: BuildingKind, pos: Vec3, team: Team, sites: &[Site], nodes: &[Vec3]) -> bool {
    let r = kind.radius();
    let lim = MAP_HALF - 8.0;
    if pos.x.abs() > lim || pos.z.abs() > lim {
        return false;
    }
    let near_base = sites
        .iter()
        .any(|s| s.team == team && flat(s.pos).distance(flat(pos)) < BUILD_RANGE + s.radius);
    if !near_base {
        return false;
    }
    if sites.iter().any(|s| flat(s.pos).distance(flat(pos)) < r + s.radius + 1.5) {
        return false;
    }
    if nodes.iter().any(|n| flat(*n).distance(flat(pos)) < r + 4.0) {
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
pub fn spawn_structure(c: &mut Commands, kit: &ModelKit, team: Team, kind: BuildingKind, pos: Vec3) -> Entity {
    let ground = terrain::on_ground(pos);
    let depth = kit.structures[kind.idx()][team.0 as usize].height;
    let root = c
        .spawn((
            Transform::from_translation(ground).looking_to(-facing(team), Vec3::Y),
            Visibility::default(),
            team,
            Health { hp: kind.hp() * 0.1, max: kind.hp() },
            Building { radius: kind.radius() },
            Structure(kind),
            Construction { progress: 0.0, dust: 0.0 },
        ))
        .id();
    let pivot = c.spawn((Transform::from_xyz(0.0, -depth, 0.0), Visibility::default())).id();
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
    }
    root
}

fn collect_sites(q: &Query<(&Transform, &Building, &Team)>) -> Vec<Site> {
    q.iter().map(|(t, b, team)| Site { pos: t.translation, radius: b.radius, team: *team }).collect()
}

/// Baumodus des Spielers: Tasten, Vorschau, Klick
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
    mut reqs: MessageWriter<PlaceRequest>,
    sites: Query<(&Transform, &Building, &Team)>,
    nodes: Query<&Transform, With<ResourceNode>>,
    mut ghosts: Query<(&mut Transform, &mut Visibility), (Without<Building>, Without<ResourceNode>)>,
    mut gizmos: Gizmos,
) {
    for (key, kind) in [
        (KeyCode::Digit7, BuildingKind::Geschuetzturm),
        (KeyCode::Digit8, BuildingKind::Raffinerie),
        (KeyCode::Digit9, BuildingKind::Fabrik),
    ] {
        if keys.just_pressed(key) {
            placing.kind = if placing.kind == Some(kind) { None } else { Some(kind) };
        }
    }
    if placing.kind.is_some() && (keys.just_pressed(KeyCode::Escape) || mouse.just_pressed(MouseButton::Right)) {
        placing.kind = None;
        placing.block_click = true;
    }

    let Some(kind) = placing.kind else {
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
    let valid = point.is_some_and(|p| can_place(kind, p, PLAYER, &site_list, &node_list) && stock.can_afford(PLAYER, kind.cost()));

    // Vorschau neu aufbauen, wenn sich Typ oder Gültigkeit ändert
    if placing.ghost.is_none_or(|(_, k, v)| k != kind || v != valid) {
        if let Some((g, _, _)) = placing.ghost.take() {
            commands.entity(g).try_despawn();
        }
        let root = commands
            .spawn((
                Transform::from_translation(terrain::on_ground(point.unwrap_or_default())).looking_to(-facing(PLAYER), Vec3::Y),
                if point.is_some() { Visibility::Visible } else { Visibility::Hidden },
            ))
            .id();
        let mat = &kit.ghost[if valid { 0 } else { 1 }];
        models::build_structure(&mut commands, &kit, root, PLAYER, kind, Some(mat));
        placing.ghost = Some((root, kind, valid));
    } else if let Some((g, _, _)) = placing.ghost
        && let Ok((mut t, mut vis)) = ghosts.get_mut(g)
    {
        match point {
            Some(p) => {
                t.translation = terrain::on_ground(p);
                *vis = Visibility::Visible;
            }
            None => *vis = Visibility::Hidden,
        }
    }

    let Some(p) = point else { return };
    // Bauradius um eigene Gebäude und Grundfläche
    let col = if valid { Color::srgb(0.4, 1.0, 0.5) } else { Color::srgb(1.0, 0.35, 0.3) };
    let ground = terrain::on_ground(p) + Vec3::Y * 0.2;
    gizmos.circle(Isometry3d::new(ground, Quat::from_rotation_x(PI / 2.0)), kind.radius(), col);
    for s in site_list.iter().filter(|s| s.team == PLAYER) {
        let c = terrain::on_ground(s.pos) + Vec3::Y * 0.3;
        gizmos.circle(Isometry3d::new(c, Quat::from_rotation_x(PI / 2.0)), BUILD_RANGE + s.radius, Color::srgba(0.5, 0.8, 1.0, 0.25));
    }

    if mouse.just_pressed(MouseButton::Left) {
        placing.block_click = true;
        if valid {
            reqs.write(PlaceRequest { team: PLAYER, kind, pos: p });
            if !keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
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
    sites: Query<(&Transform, &Building, &Team)>,
    nodes: Query<&Transform, With<ResourceNode>>,
    mut fx: MessageWriter<FxEvent>,
) {
    let mut site_list = collect_sites(&sites);
    let node_list: Vec<Vec3> = nodes.iter().map(|t| t.translation).collect();
    for r in reqs.read() {
        if !stock.can_afford(r.team, r.kind.cost()) || !can_place(r.kind, r.pos, r.team, &site_list, &node_list) {
            continue;
        }
        stock.pay(r.team, r.kind.cost());
        spawn_structure(&mut commands, &kit, r.team, r.kind, r.pos);
        site_list.push(Site { pos: r.pos, radius: r.kind.radius(), team: r.team });
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
        let want = match atk.and_then(|a| snap.0.get(&a.0)) {
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
