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
    mut wind_mats: ResMut<Assets<crate::vegetation::WindMaterial>>,
) {
    let kit = ModelKit::new(&mut meshes, &mut mats);
    commands.insert_resource(FxKit::new(&mut meshes, &mut mats, &mut images));

    // Das Gelände selbst entsteht in splat::finish_terrain, sobald die Texturen geladen sind

    // Sonne: tief stehend und warm, für lange Schatten
    commands.spawn((
        DirectionalLight {
            illuminance: 11_500.0,
            color: Color::srgb(1.0, 0.89, 0.74),
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_translation(crate::sky::sun_dir() * 60.0).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            maximum_distance: 140.0,
            first_cascade_far_bound: 140.0,
            ..default()
        }
        .build(),
    ));
    // Das meiste Umgebungslicht kommt jetzt aus der Himmels-Umgebungskarte (sky.rs);
    // ein kleiner Grundwert verhindert ganz schwarze Ecken
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.62, 0.70, 0.85),
        brightness: 90.0,
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
    // Wind-Material ohne Wind: so bekommen auch die Felsen Wolkenschatten und Nebel des Krieges
    let rock_mat = wind_mats.add(bevy::pbr::ExtendedMaterial {
        base: StandardMaterial { perceptual_roughness: 0.93, ..default() },
        extension: crate::vegetation::WindExt::new(Vec4::ZERO),
    });
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
        if FIELDS.iter().any(|(f, _, _)| p.distance(*f) < 6.0) || terrain::track_weight(x, z) > 0.1 {
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
    // Kiesel: viele kleine Steine, gehäuft an Hangfüßen, Wegrändern und in Kratern
    let mut pebbles = 0;
    while pebbles < 700 {
        let x = rng.range(-MAP_HALF, MAP_HALF);
        let z = rng.range(-MAP_HALF, MAP_HALF);
        let p = Vec3::new(x, 0.0, z);
        if p.distance(PLAYER_HQ_POS) < 12.0 || p.distance(ENEMY_HQ_POS) < 12.0 || FIELDS.iter().any(|(f, _, _)| p.distance(*f) < 4.0) {
            continue;
        }
        let w = terrain::weights_at(x, z);
        let track = terrain::track_weight(x, z);
        // auf der Fahrbahn selbst liegen keine Steine, am Rand und im Geröll viele
        let chance = (w[1] * 0.8 + (track * (1.0 - track) * 3.0) + w[2] * 0.4).min(1.0) * (1.0 - track.powi(4));
        if rng.next_f32() > chance {
            continue;
        }
        pebbles += 1;
        let s = rng.range(0.07, 0.24);
        let g = terrain::on_ground(p);
        let tint = rock_tints[(rng.next_f32() * 3.0) as usize % 3] * rng.range(0.85, 1.25);
        let tf = Transform::from_translation(g - Vec3::Y * s * 0.25)
            .with_scale(Vec3::new(s * rng.range(0.8, 1.5), s * rng.range(0.6, 1.0), s * rng.range(0.8, 1.5)))
            .with_rotation(Quat::from_rotation_y(rng.range(0.0, std::f32::consts::TAU)));
        let mut m = rock_shapes[(rng.next_f32() * 6.0) as usize % 6].clone().transformed_by(tf);
        let n = m.count_vertices();
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![tint.to_f32_array(); n]);
        match &mut chunks[chunk_of(g)] {
            Some(acc) => {
                let _ = acc.merge(&m);
            }
            slot => *slot = Some(m),
        }
    }

    // Felsformationen am Rand des Spielfelds: geschichtete Tafelberge und Felsnadeln
    let bands = [
        LinearRgba::from(Color::srgb(0.46, 0.38, 0.29)),
        LinearRgba::from(Color::srgb(0.36, 0.32, 0.28)),
        LinearRgba::from(Color::srgb(0.52, 0.43, 0.33)),
        LinearRgba::from(Color::srgb(0.30, 0.27, 0.25)),
    ];
    let mut formations = 0;
    let mut tries = 0;
    while formations < 18 && tries < 2000 {
        tries += 1;
        let x = rng.range(-MAP_HALF - 24.0, MAP_HALF + 24.0);
        let z = rng.range(-MAP_HALF - 24.0, MAP_HALF + 24.0);
        let edge = x.abs().max(z.abs());
        if !(MAP_HALF + 2.0..MAP_HALF + 24.0).contains(&edge) {
            continue;
        }
        let base = terrain::on_ground(Vec3::new(x, 0.0, z));
        let spire = rng.next_f32() < 0.35;
        let layers = if spire { 4 + (rng.next_f32() * 3.0) as usize } else { 3 + (rng.next_f32() * 3.0) as usize };
        let mut width = if spire { rng.range(2.0, 3.2) } else { rng.range(5.0, 9.0) };
        let mut y = base.y - 1.5;
        let yaw = rng.range(0.0, std::f32::consts::TAU);
        let mut off = Vec3::ZERO;
        for layer in 0..layers {
            let h = rng.range(1.4, 2.6) * if spire { 1.3 } else { 1.0 };
            let tf = Transform::from_translation(Vec3::new(x, y, z) + off)
                .with_scale(Vec3::new(width * rng.range(0.85, 1.15), h, width * rng.range(0.7, 1.0)))
                .with_rotation(Quat::from_rotation_y(yaw + rng.range(-0.3, 0.3)));
            let mut m = rock_shapes[(layer + formations) % 6].clone().transformed_by(tf);
            let n = m.count_vertices();
            let tint = bands[(layer + formations) % 4] * rng.range(0.9, 1.1);
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![tint.to_f32_array(); n]);
            match &mut chunks[chunk_of(base)] {
                Some(acc) => {
                    let _ = acc.merge(&m);
                }
                slot => *slot = Some(m),
            }
            y += h * 0.78;
            width *= if spire { rng.range(0.78, 0.95) } else { rng.range(0.82, 0.97) };
            off += Vec3::new(rng.range(-0.3, 0.3), 0.0, rng.range(-0.3, 0.3));
        }
        formations += 1;
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

    spawn_start_forces(&mut commands, &kit);

    commands.insert_resource(kit);
}

/// Hauptquartiere und Startarmeen beider Seiten
pub fn spawn_start_forces(commands: &mut Commands, kit: &ModelKit) {
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
        models::build_hq(commands, kit, hq, team);
    }

    // Startarmeen
    for i in 0..2 {
        let o = i as f32 * 2.8;
        spawn_unit(commands, kit, PLAYER, UnitKind::Sammler, PLAYER_HQ_POS + Vec3::new(7.0 + o, 0.0, -7.0));
        spawn_unit(commands, kit, ENEMY, UnitKind::Sammler, ENEMY_HQ_POS + Vec3::new(-7.0 - o, 0.0, 7.0));
    }
    for i in 0..3 {
        let o = i as f32 * 2.8;
        spawn_unit(commands, kit, PLAYER, UnitKind::Panzer, PLAYER_HQ_POS + Vec3::new(9.0 + o, 0.0, -1.0));
        spawn_unit(commands, kit, ENEMY, UnitKind::Panzer, ENEMY_HQ_POS + Vec3::new(-9.0 - o, 0.0, 1.0));
    }
}

pub fn spawn_unit(commands: &mut Commands, kit: &ModelKit, team: Team, kind: UnitKind, pos: Vec3) -> Entity {
    let gun = |range: f32, damage: f32, cooldown: f32, projectile_speed: f32| Weapon {
        range,
        damage,
        cooldown,
        timer: 0.0,
        projectile_speed,
        rocket: false,
        splash: 0.0,
        arc: 0.0,
    };
    let (hp, speed, radius, weapon) = match kind {
        UnitKind::Sammler => (160.0, 7.0, 1.3, None),
        UnitKind::Panzer => (220.0, 6.0, 1.4, Some(gun(11.0, 18.0, 1.1, 38.0))),
        UnitKind::Werfer => (140.0, 4.8, 1.3, Some(Weapon { rocket: true, ..gun(19.0, 34.0, 2.4, 20.0) })),
        UnitKind::Spaeher => (110.0, 10.0, 1.3, Some(gun(12.0, 6.0, 0.3, 60.0))),
        UnitKind::Bollwerk => (520.0, 4.0, 1.9, Some(gun(13.0, 48.0, 2.0, 40.0))),
        UnitKind::Haubitze => (150.0, 4.2, 1.5, Some(Weapon { splash: 3.5, arc: 0.28, ..gun(27.0, 42.0, 4.5, 22.0) })),
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
    e.insert(Motion { prev: pos, ..default() });
    let id = e.id();
    models::build_unit(commands, kit, id, team, kind);
    id
}
