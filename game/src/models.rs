//! Fahrzeug- und Gebäudemodelle, zusammengesetzt aus Grundformen mit PBR-Materialien.
//! Vorne ist bei allen Modellen -Z, der Ursprung liegt auf dem Boden.

use crate::components::*;
use crate::buildings::{self, HqMeshes, Spin};
use crate::vehicles::{self, VehicleMeshes};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

#[derive(Resource)]
pub struct ModelKit {
    pub cyl: Handle<Mesh>,
    pub tracer: Handle<Mesh>,
    pub crystal: Handle<Mesh>,
    pub ring: Handle<Mesh>,
    pub glow: [Handle<StandardMaterial>; 2],
    pub tracer_mat: [Handle<StandardMaterial>; 2],
    pub rocket_mat: [Handle<StandardMaterial>; 2],
    pub glass: Handle<StandardMaterial>,
    pub ferrit: Handle<StandardMaterial>,
    pub plasma: Handle<StandardMaterial>,
    pub crater: Handle<StandardMaterial>,
    /// Detailmodelle je Typ (Sammler, Panzer, Werfer) und Team
    pub vehicles: [[VehicleMeshes; 2]; 3],
    /// Fahrzeuglack: Farbe kommt aus den Vertexfarben
    pub vehicle_mat: Handle<StandardMaterial>,
    pub hq: [HqMeshes; 2],
    /// Beton und Bauteile der Gebäude (Farbe aus den Vertexfarben)
    pub building_mat: Handle<StandardMaterial>,
}

impl ModelKit {
    pub fn new(meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> Self {
        let glow = |m: &mut Assets<StandardMaterial>, c: LinearRgba| {
            m.add(StandardMaterial {
                base_color: Color::BLACK,
                emissive: c,
                ..default()
            })
        };
        // Kristall: Sechskantprisma mit Spitze
        let mut crystal = Cylinder::new(0.28, 1.6).mesh().resolution(6).build().translated_by(Vec3::Y * 0.8);
        crystal
            .merge(&Cone::new(0.28, 0.55).mesh().resolution(6).build().translated_by(Vec3::Y * 1.875))
            .expect("Kristall-Mesh");

        Self {
            cyl: meshes.add(Cylinder::new(0.5, 1.0).mesh().resolution(20)),
            tracer: meshes.add(Capsule3d::new(0.05, 0.8)),
            crystal: meshes.add(crystal),
            ring: meshes.add(Torus::new(1.3, 2.2)),
            glow: [
                glow(mats, LinearRgba::rgb(0.6, 4.0, 9.0)),
                glow(mats, LinearRgba::rgb(9.0, 0.8, 0.3)),
            ],
            tracer_mat: [
                glow(mats, LinearRgba::rgb(6.0, 10.0, 14.0)),
                glow(mats, LinearRgba::rgb(16.0, 6.0, 2.0)),
            ],
            rocket_mat: [
                glow(mats, LinearRgba::rgb(8.0, 6.0, 3.0)),
                glow(mats, LinearRgba::rgb(14.0, 3.0, 1.0)),
            ],
            glass: mats.add(StandardMaterial {
                base_color: Color::srgb(0.05, 0.09, 0.14),
                perceptual_roughness: 0.06,
                reflectance: 0.9,
                ..default()
            }),
            ferrit: mats.add(StandardMaterial {
                base_color: Color::srgb(0.25, 0.85, 0.7),
                emissive: LinearRgba::rgb(0.25, 2.2, 1.5),
                metallic: 0.1,
                perceptual_roughness: 0.12,
                reflectance: 0.8,
                ..default()
            }),
            plasma: mats.add(StandardMaterial {
                base_color: Color::BLACK,
                emissive: LinearRgba::rgb(6.0, 1.4, 9.0),
                ..default()
            }),
            crater: mats.add(StandardMaterial {
                base_color: Color::srgb(0.20, 0.18, 0.17),
                perceptual_roughness: 0.95,
                ..default()
            }),
            vehicles: [
                [vehicles::build_sammler(PLAYER, meshes), vehicles::build_sammler(ENEMY, meshes)],
                [vehicles::build_panzer(PLAYER, meshes), vehicles::build_panzer(ENEMY, meshes)],
                [vehicles::build_werfer(PLAYER, meshes), vehicles::build_werfer(ENEMY, meshes)],
            ],
            vehicle_mat: mats.add(StandardMaterial {
                perceptual_roughness: 0.58,
                metallic: 0.22,
                reflectance: 0.45,
                ..default()
            }),
            hq: [buildings::build_hq(PLAYER, meshes), buildings::build_hq(ENEMY, meshes)],
            building_mat: mats.add(StandardMaterial {
                perceptual_roughness: 0.8,
                metallic: 0.08,
                reflectance: 0.35,
                ..default()
            }),
        }
    }
}

/// Ein Bauteil als Kind von `parent`
fn part(
    c: &mut Commands,
    parent: Entity,
    mesh: &Handle<Mesh>,
    mat: &Handle<StandardMaterial>,
    t: Transform,
) -> Entity {
    let e = c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), t)).id();
    c.entity(parent).add_child(e);
    e
}

fn tf(x: f32, y: f32, z: f32, sx: f32, sy: f32, sz: f32) -> Transform {
    Transform::from_xyz(x, y, z).with_scale(Vec3::new(sx, sy, sz))
}

/// Baut das Modell an `root` und gibt den Turm (falls vorhanden) zurück
pub fn build_unit(c: &mut Commands, k: &ModelKit, root: Entity, team: Team, kind: UnitKind) -> Option<Entity> {
    let t = team.0 as usize;
    let (vm, turret_base) = match kind {
        UnitKind::Sammler => (&k.vehicles[0][t], None),
        UnitKind::Panzer => (&k.vehicles[1][t], Some(Turret { tilt: 0.0, base_y: 0.86, base_z: 0.1 })),
        UnitKind::Werfer => (&k.vehicles[2][t], Some(Turret { tilt: 0.35, base_y: 1.05, base_z: 0.25 })),
    };
    part(c, root, &vm.hull, &k.vehicle_mat, Transform::default());
    if let Some(m) = &vm.hull_glow {
        part(c, root, m, &k.glow[t], Transform::default());
    }
    if let Some(m) = &vm.glass {
        part(c, root, m, &k.glass, Transform::default());
    }
    let turret = turret_base?;
    let pivot = c
        .spawn((Transform::from_xyz(0.0, turret.base_y, turret.base_z), Visibility::default(), turret))
        .id();
    c.entity(root).add_child(pivot);
    if let Some(m) = &vm.turret {
        part(c, pivot, m, &k.vehicle_mat, Transform::default());
    }
    if let Some(m) = &vm.turret_glow {
        let mat = if kind == UnitKind::Werfer { &k.rocket_mat[t] } else { &k.glow[t] };
        part(c, pivot, m, mat, Transform::default());
    }
    Some(pivot)
}

/// Hauptquartier
pub fn build_hq(c: &mut Commands, k: &ModelKit, root: Entity, team: Team) {
    let t = team.0 as usize;
    let hq = &k.hq[t];
    part(c, root, &hq.body, &k.building_mat, Transform::default());
    part(c, root, &hq.glow, &k.glow[t], Transform::default());
    part(c, root, &hq.glass, &k.glass, Transform::default());
    let dish = part(c, root, &hq.dish, &k.building_mat, Transform::from_translation(hq.dish_pos));
    c.entity(dish).insert(Spin(1.2));
    let light = c
        .spawn((
            PointLight {
                color: if t == 0 { Color::srgb(0.5, 0.8, 1.0) } else { Color::srgb(1.0, 0.35, 0.25) },
                intensity: 900_000.0,
                range: 16.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(0.0, 3.0, 3.5),
        ))
        .id();
    c.entity(root).add_child(light);
}

/// Ferrit-Kristallgruppe
pub fn build_ferrit(c: &mut Commands, k: &ModelKit, root: Entity, seed: u32) {
    let mut rng = Rng(0xA5A5_1234 ^ (seed as u64 * 7919 + 1));
    for i in 0..9 {
        let ang = i as f32 * 2.4 + rng.range(-0.3, 0.3);
        let r = if i == 0 { 0.0 } else { rng.range(0.6, 2.0) };
        let s = if i == 0 { 1.5 } else { rng.range(0.55, 1.15) };
        let tilt = if i == 0 { 0.05 } else { rng.range(0.2, 0.6) };
        let rot = Quat::from_rotation_y(ang) * Quat::from_rotation_x(tilt);
        part(
            c,
            root,
            &k.crystal,
            &k.ferrit,
            Transform::from_xyz(ang.cos() * r, -0.15, ang.sin() * r)
                .with_rotation(rot)
                .with_scale(Vec3::splat(s)),
        );
    }
    let light = c
        .spawn((
            PointLight {
                color: Color::srgb(0.4, 1.0, 0.8),
                intensity: 250_000.0,
                range: 9.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(0.0, 2.2, 0.0),
        ))
        .id();
    c.entity(root).add_child(light);
}

/// Pulsierende Lichtquelle (Plasma)
#[derive(Component)]
pub struct Pulse {
    pub base: f32,
    pub phase: f32,
}

/// Plasmaquelle: Kraterring mit leuchtendem Becken
pub fn build_plasma(c: &mut Commands, k: &ModelKit, root: Entity, seed: u32) {
    part(c, root, &k.ring, &k.crater, tf(0.0, 0.05, 0.0, 1.0, 1.6, 1.0));
    let pool = part(c, root, &k.cyl, &k.plasma, tf(0.0, 0.08, 0.0, 2.8, 0.1, 2.8));
    c.entity(pool).insert(NotShadowCaster);
    let light = c
        .spawn((
            PointLight {
                color: Color::srgb(0.8, 0.35, 1.0),
                intensity: 700_000.0,
                range: 12.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(0.0, 1.5, 0.0),
            Pulse { base: 700_000.0, phase: seed as f32 * 1.7 },
        ))
        .id();
    c.entity(root).add_child(light);
}

pub fn pulse_lights(time: Res<Time>, mut q: Query<(&Pulse, &mut PointLight)>) {
    let t = time.elapsed_secs();
    for (p, mut l) in &mut q {
        l.intensity = p.base * (0.75 + 0.25 * (t * 2.1 + p.phase).sin() + 0.08 * (t * 7.3 + p.phase).sin());
    }
}

/// Kristallgruppen schrumpfen, wenn sie abgebaut werden
pub fn shrink_nodes(mut q: Query<(&ResourceNode, &mut Transform), Changed<ResourceNode>>) {
    for (n, mut t) in &mut q {
        if n.kind == ResKind::Ferrit {
            let r = n.amount as f32 / n.max.max(1) as f32;
            t.scale = Vec3::splat(0.35 + 0.65 * r);
        }
    }
}
