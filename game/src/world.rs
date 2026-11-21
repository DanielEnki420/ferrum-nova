use crate::components::*;
use crate::fx::FxKit;
use crate::models::{self, ModelKit};
use crate::resources;
use crate::terrain::{self, FIELDS};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;

pub fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let kit = ModelKit::new(&mut meshes, &mut mats);
    commands.insert_resource(FxKit::new(&mut meshes, &mut mats, &mut images));

    // Das Gelände selbst entsteht in splat::finish_terrain, sobald die Texturen geladen sind

    // Sonne: tief stehend und warm, für lange Schatten
    commands.spawn((
        DirectionalLight {
            illuminance: 11_000.0,
            color: Color::srgb(1.0, 0.93, 0.82),
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-40.0, 38.0, 30.0).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            maximum_distance: 140.0,
            first_cascade_far_bound: 140.0,
            ..default()
        }
        .build(),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.62, 0.70, 0.85),
        brightness: 420.0,
        ..default()
    });

    // Felsen und Geröll: statisch, daher pro Kartenkachel zu einem einzigen Mesh
    // zusammengefasst (WebGL2 zahlt für jeden Zeichenaufruf, im Schattendurchgang doppelt).
    // Die Farbvarianten stecken in den Vertexfarben, so reicht ein Material.
    let rock_shapes: Vec<Mesh> = (0..6).map(|s| terrain::rock_mesh(s + 1)).collect();
    let rock_tints = [
        LinearRgba::from(Color::srgb(0.34, 0.31, 0.27)),
        LinearRgba::from(Color::srgb(0.27, 0.25, 0.23)),
        LinearRgba::from(Color::srgb(0.42, 0.37, 0.30)),
    ];
    let rock_mat = mats.add(StandardMaterial { perceptual_roughness: 0.93, ..default() });
    const CHUNKS: usize = 5;
    let chunk_of = |p: Vec3| {
        let f = |v: f32| (((v + terrain::TERRAIN_HALF) / (2.0 * terrain::TERRAIN_HALF) * CHUNKS as f32) as usize).min(CHUNKS - 1);
        f(p.z) * CHUNKS + f(p.x)
    };
    let mut chunks: Vec<Option<Mesh>> = (0..CHUNKS * CHUNKS).map(|_| None).collect();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut placed = 0;
    while placed < 230 {
        let edge_zone = placed >= 140;
        let lim = if edge_zone { terrain::TERRAIN_HALF - 8.0 } else { MAP_HALF - 3.0 };
        let x = rng.range(-lim, lim);
        let z = rng.range(-lim, lim);
        let p = Vec3::new(x, 0.0, z);
        if edge_zone && x.abs().max(z.abs()) < MAP_HALF {
            continue;
        }
        if p.distance(PLAYER_HQ_POS) < 17.0 || p.distance(ENEMY_HQ_POS) < 17.0 {
            continue;
        }
        if FIELDS.iter().any(|(f, _, _)| p.distance(*f) < 6.0) {
            continue;
        }
        placed += 1;
        // viele kleine Steine, wenige große Brocken
        let r = rng.next_f32();
        let s = if r < 0.55 { rng.range(0.25, 0.7) } else if r < 0.9 { rng.range(0.8, 1.8) } else { rng.range(2.0, 4.0) };
        let s = if edge_zone { s * 2.2 } else { s };
        let g = terrain::on_ground(p);
        let shape = (rng.next_f32() * 6.0) as usize % 6;
        let tint = rock_tints[(rng.next_f32() * 3.0) as usize % 3];
        let tf = Transform::from_translation(g - Vec3::Y * s * 0.15)
            .with_scale(Vec3::new(s * rng.range(0.8, 1.3), s, s * rng.range(0.8, 1.3)))
            .with_rotation(Quat::from_rotation_y(rng.range(0.0, std::f32::consts::TAU)));
        let mut m = rock_shapes[shape].clone().transformed_by(tf);
        let n = m.count_vertices();
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![tint.to_f32_array(); n]);
        match &mut chunks[chunk_of(g)] {
            Some(acc) => {
                let _ = acc.merge(&m);
            }
            slot => *slot = Some(m),
        }
    }
    for m in chunks.into_iter().flatten() {
        commands.spawn((Mesh3d(meshes.add(m)), MeshMaterial3d(rock_mat.clone())));
    }

    // Rohstofffelder
    for (i, (pos, kind, amount)) in FIELDS.into_iter().enumerate() {
        let root = commands
            .spawn((
                Transform::from_translation(terrain::on_ground(pos)),
                Visibility::default(),
                ResourceNode { kind, amount, max: amount },
            ))
            .id();
        match kind {
            ResKind::Ferrit => resources::spawn_ferrit(&mut commands, &kit, &mut meshes, root, i as u32),
            ResKind::Plasma => resources::spawn_plasma(&mut commands, &kit, &mut meshes, root, i as u32),
        }
    }

    // Hauptquartiere
    for (team, pos) in [(PLAYER, PLAYER_HQ_POS), (ENEMY, ENEMY_HQ_POS)] {
        let facing = if team == PLAYER { Vec3::new(1.0, 0.0, -1.0) } else { Vec3::new(-1.0, 0.0, 1.0) };
        let hq = commands
            .spawn((
                Transform::from_translation(terrain::on_ground(pos)).looking_to(-facing, Vec3::Y),
                Visibility::default(),
                team,
                Health { hp: 3000.0, max: 3000.0 },
                Building { radius: 4.8 },
                Hq { queue: Vec::new(), progress: 0.0 },
                Depot,
            ))
            .id();
        models::build_hq(&mut commands, &kit, hq, team);
    }

    // Startarmeen
    for i in 0..2 {
        let o = i as f32 * 2.8;
        spawn_unit(&mut commands, &kit, PLAYER, UnitKind::Sammler, PLAYER_HQ_POS + Vec3::new(7.0 + o, 0.0, -7.0));
        spawn_unit(&mut commands, &kit, ENEMY, UnitKind::Sammler, ENEMY_HQ_POS + Vec3::new(-7.0 - o, 0.0, 7.0));
    }
    for i in 0..3 {
        let o = i as f32 * 2.8;
        spawn_unit(&mut commands, &kit, PLAYER, UnitKind::Panzer, PLAYER_HQ_POS + Vec3::new(9.0 + o, 0.0, -1.0));
        spawn_unit(&mut commands, &kit, ENEMY, UnitKind::Panzer, ENEMY_HQ_POS + Vec3::new(-9.0 - o, 0.0, 1.0));
    }

    commands.insert_resource(kit);
}

pub fn spawn_unit(commands: &mut Commands, kit: &ModelKit, team: Team, kind: UnitKind, pos: Vec3) -> Entity {
    let (hp, speed, radius, weapon) = match kind {
        UnitKind::Sammler => (160.0, 7.0, 1.3, None),
        UnitKind::Panzer => (
            220.0,
            6.0,
            1.4,
            Some(Weapon { range: 11.0, damage: 18.0, cooldown: 1.1, timer: 0.0, projectile_speed: 38.0, rocket: false }),
        ),
        UnitKind::Werfer => (
            140.0,
            4.8,
            1.3,
            Some(Weapon { range: 19.0, damage: 34.0, cooldown: 2.4, timer: 0.0, projectile_speed: 20.0, rocket: true }),
        ),
    };
    let facing = if team == PLAYER { Vec3::new(1.0, 0.0, -1.0) } else { Vec3::new(-1.0, 0.0, 1.0) };
    let heading = Heading::from_dir(facing);
    let mut e = commands.spawn((
        Transform::from_translation(terrain::on_ground(pos)).with_rotation(Quat::from_rotation_y(heading.0)),
        Visibility::default(),
        team,
        Health { hp, max: hp },
        Unit { kind, speed, radius },
        heading,
    ));
    if let Some(w) = weapon {
        e.insert(w);
    }
    if kind == UnitKind::Sammler {
        e.insert(Harvester {
            state: HarvestState::Idle,
            node: None,
            cargo: 0,
            cargo_kind: ResKind::Ferrit,
        });
    }
    let id = e.id();
    if let Some(turret) = models::build_unit(commands, kit, id, team, kind) {
        commands.entity(id).insert(TurretRef(turret));
    }
    id
}
