//! Fahrzeug- und Gebäudemodelle, zusammengesetzt aus Grundformen mit PBR-Materialien.
//! Vorne ist bei allen Modellen -Z, der Ursprung liegt auf dem Boden.

use crate::components::*;
use crate::buildings::{self, HqMeshes, Spin};
use crate::vehicles::{self, VehicleMeshes};
use bevy::prelude::*;

#[derive(Resource)]
pub struct ModelKit {
    pub tracer: Handle<Mesh>,
    pub disc: Handle<Mesh>,
    pub bubble: Handle<Mesh>,
    pub glow: [Handle<StandardMaterial>; 2],
    pub tracer_mat: [Handle<StandardMaterial>; 2],
    pub rocket_mat: [Handle<StandardMaterial>; 2],
    pub glass: Handle<StandardMaterial>,
    pub ferrit: Handle<StandardMaterial>,
    pub plasma: Handle<StandardMaterial>,
    pub ferrit_glow: Handle<StandardMaterial>,
    pub plasma_core: Handle<StandardMaterial>,
    pub plasma_crystal: Handle<StandardMaterial>,
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
        Self {
            tracer: meshes.add(Capsule3d::new(0.05, 0.8)),
            disc: meshes.add(Cylinder::new(1.0, 0.02).mesh().resolution(32)),
            bubble: meshes.add(Sphere::new(1.0).mesh().ico(2).expect("Kugel")),
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
            // Farbe aus den Vertexfarben (unten tief, an den Spitzen hell)
            ferrit: mats.add(StandardMaterial {
                emissive: LinearRgba::rgb(0.06, 0.75, 0.5),
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
            ferrit_glow: glow(mats, LinearRgba::rgb(0.4, 6.0, 4.0)),
            plasma_core: glow(mats, LinearRgba::rgb(14.0, 5.0, 18.0)),
            plasma_crystal: mats.add(StandardMaterial {
                emissive: LinearRgba::rgb(0.9, 0.15, 1.4),
                metallic: 0.1,
                perceptual_roughness: 0.15,
                reflectance: 0.8,
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

/// Pulsierende Lichtquelle (Plasma)
#[derive(Component)]
pub struct Pulse {
    pub base: f32,
    pub phase: f32,
}

pub fn pulse_lights(time: Res<Time>, mut q: Query<(&Pulse, &mut PointLight)>) {
    let t = time.elapsed_secs();
    for (p, mut l) in &mut q {
        l.intensity = p.base * (0.75 + 0.25 * (t * 2.1 + p.phase).sin() + 0.08 * (t * 7.3 + p.phase).sin());
    }
}
