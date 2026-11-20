use crate::components::*;
use bevy::prelude::*;

pub fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let assets = Assets3d {
        sammler: meshes.add(Cuboid::new(1.6, 1.0, 2.2)),
        panzer: meshes.add(Cuboid::new(1.8, 0.8, 2.4)),
        werfer: meshes.add(Cuboid::new(1.6, 1.4, 1.6)),
        turret: meshes.add(Cuboid::new(0.7, 0.5, 1.4)),
        projectile: meshes.add(Sphere::new(0.25)),
        team_mat: [
            mats.add(StandardMaterial {
                base_color: Color::srgb(0.25, 0.55, 0.95),
                perceptual_roughness: 0.6,
                ..default()
            }),
            mats.add(StandardMaterial {
                base_color: Color::srgb(0.85, 0.2, 0.25),
                perceptual_roughness: 0.6,
                ..default()
            }),
        ],
        accent_mat: [
            mats.add(Color::srgb(0.75, 0.85, 1.0)),
            mats.add(Color::srgb(0.3, 0.05, 0.08)),
        ],
        projectile_mat: [
            mats.add(StandardMaterial {
                base_color: Color::srgb(0.6, 0.9, 1.0),
                emissive: LinearRgba::rgb(2.0, 5.0, 8.0),
                ..default()
            }),
            mats.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.5, 0.3),
                emissive: LinearRgba::rgb(8.0, 2.0, 0.5),
                ..default()
            }),
        ],
    };

    // Boden
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(MAP_HALF * 2.0, MAP_HALF * 2.0))),
        MeshMaterial3d(mats.add(StandardMaterial {
            base_color: Color::srgb(0.32, 0.30, 0.24),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::default(),
    ));

    // Licht
    commands.spawn((
        DirectionalLight {
            illuminance: 9000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(30.0, 60.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: 350.0,
        ..default()
    });

    // Felsen als Deko
    let rock_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let rock_mat = mats.add(Color::srgb(0.22, 0.21, 0.19));
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..40 {
        let x = rng.range(-MAP_HALF + 5.0, MAP_HALF - 5.0);
        let z = rng.range(-MAP_HALF + 5.0, MAP_HALF - 5.0);
        let p = Vec3::new(x, 0.0, z);
        if p.distance(PLAYER_HQ_POS) < 18.0 || p.distance(ENEMY_HQ_POS) < 18.0 {
            continue;
        }
        let s = rng.range(1.0, 3.5);
        commands.spawn((
            Mesh3d(rock_mesh.clone()),
            MeshMaterial3d(rock_mat.clone()),
            Transform::from_xyz(x, s * 0.4, z)
                .with_scale(Vec3::new(s, s * 0.8, s * 1.2))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, std::f32::consts::PI))),
        ));
    }

    // Ressourcenfelder: symmetrisch für beide Seiten
    let ferrit_mesh = meshes.add(Cuboid::new(1.2, 2.0, 1.2));
    let ferrit_mat = mats.add(StandardMaterial {
        base_color: Color::srgb(0.3, 0.9, 0.7),
        emissive: LinearRgba::rgb(0.2, 1.2, 0.8),
        ..default()
    });
    let plasma_mesh = meshes.add(Cylinder::new(1.4, 0.6));
    let plasma_mat = mats.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.4, 1.0),
        emissive: LinearRgba::rgb(2.5, 0.6, 3.0),
        ..default()
    });
    let fields: [(Vec3, ResKind, u32); 7] = [
        (Vec3::new(-28.0, 0.0, 50.0), ResKind::Ferrit, 1500),
        (Vec3::new(-50.0, 0.0, 26.0), ResKind::Plasma, 1000),
        (Vec3::new(-15.0, 0.0, 15.0), ResKind::Ferrit, 2500),
        (Vec3::new(28.0, 0.0, -50.0), ResKind::Ferrit, 1500),
        (Vec3::new(50.0, 0.0, -26.0), ResKind::Plasma, 1000),
        (Vec3::new(15.0, 0.0, -15.0), ResKind::Ferrit, 2500),
        (Vec3::new(0.0, 0.0, 0.0), ResKind::Plasma, 3000),
    ];
    for (pos, kind, amount) in fields {
        match kind {
            ResKind::Ferrit => {
                // Kristallgruppe
                let parent = commands
                    .spawn((
                        Transform::from_translation(pos),
                        Visibility::default(),
                        ResourceNode { kind, amount },
                    ))
                    .id();
                for (i, off) in [
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(1.3, 0.7, 0.6),
                    Vec3::new(-1.0, 0.6, 1.1),
                    Vec3::new(0.4, 0.8, -1.3),
                ]
                .iter()
                .enumerate()
                {
                    let child = commands
                        .spawn((
                            Mesh3d(ferrit_mesh.clone()),
                            MeshMaterial3d(ferrit_mat.clone()),
                            Transform::from_translation(*off)
                                .with_rotation(Quat::from_euler(
                                    EulerRot::XYZ,
                                    0.2 * i as f32,
                                    i as f32,
                                    0.15,
                                ))
                                .with_scale(Vec3::splat(1.0 - 0.15 * i as f32)),
                        ))
                        .id();
                    commands.entity(parent).add_child(child);
                }
            }
            ResKind::Plasma => {
                commands.spawn((
                    Mesh3d(plasma_mesh.clone()),
                    MeshMaterial3d(plasma_mat.clone()),
                    Transform::from_translation(pos + Vec3::Y * 0.3),
                    ResourceNode { kind, amount },
                ));
            }
        }
    }

    // Hauptquartiere
    let hq_mesh = meshes.add(Cuboid::new(6.0, 3.0, 6.0));
    let tower_mesh = meshes.add(Cylinder::new(1.2, 4.0));
    for (team, pos) in [(PLAYER, PLAYER_HQ_POS), (ENEMY, ENEMY_HQ_POS)] {
        let t = team.0 as usize;
        let hq = commands
            .spawn((
                Mesh3d(hq_mesh.clone()),
                MeshMaterial3d(assets.team_mat[t].clone()),
                Transform::from_translation(pos + Vec3::Y * 1.5),
                team,
                Health {
                    hp: 3000.0,
                    max: 3000.0,
                },
                Building { radius: 4.2 },
                Hq {
                    queue: Vec::new(),
                    progress: 0.0,
                },
            ))
            .id();
        let tower = commands
            .spawn((
                Mesh3d(tower_mesh.clone()),
                MeshMaterial3d(assets.accent_mat[t].clone()),
                Transform::from_xyz(1.5, 3.0, 1.5),
            ))
            .id();
        commands.entity(hq).add_child(tower);
    }

    // Startarmeen
    for i in 0..2 {
        spawn_unit(&mut commands, &assets, PLAYER, UnitKind::Sammler, PLAYER_HQ_POS + Vec3::new(6.0 + i as f32 * 2.5, 0.0, -6.0));
        spawn_unit(&mut commands, &assets, ENEMY, UnitKind::Sammler, ENEMY_HQ_POS + Vec3::new(-6.0 - i as f32 * 2.5, 0.0, 6.0));
    }
    for i in 0..3 {
        spawn_unit(&mut commands, &assets, PLAYER, UnitKind::Panzer, PLAYER_HQ_POS + Vec3::new(8.0 + i as f32 * 2.5, 0.0, -1.0));
        spawn_unit(&mut commands, &assets, ENEMY, UnitKind::Panzer, ENEMY_HQ_POS + Vec3::new(-8.0 - i as f32 * 2.5, 0.0, 1.0));
    }

    commands.insert_resource(assets);
}

pub fn spawn_unit(commands: &mut Commands, a: &Assets3d, team: Team, kind: UnitKind, pos: Vec3) -> Entity {
    let t = team.0 as usize;
    let (mesh, hp, speed, radius, weapon, y) = match kind {
        UnitKind::Sammler => (a.sammler.clone(), 160.0, 7.0, 1.2, None, 0.5),
        UnitKind::Panzer => (
            a.panzer.clone(),
            220.0,
            6.0,
            1.3,
            Some(Weapon { range: 11.0, damage: 18.0, cooldown: 1.1, timer: 0.0, projectile_speed: 30.0 }),
            0.4,
        ),
        UnitKind::Werfer => (
            a.werfer.clone(),
            140.0,
            4.8,
            1.2,
            Some(Weapon { range: 19.0, damage: 34.0, cooldown: 2.4, timer: 0.0, projectile_speed: 18.0 }),
            0.7,
        ),
    };
    let facing = if team == PLAYER { Vec3::new(1.0, 0.0, -1.0) } else { Vec3::new(-1.0, 0.0, 1.0) };
    let mut e = commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(a.team_mat[t].clone()),
        Transform::from_translation(flat(pos) + Vec3::Y * y).looking_to(facing, Vec3::Y),
        team,
        Health { hp, max: hp },
        Unit { kind, speed, radius },
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
    // Aufbau (Turm / Ladefläche) in Akzentfarbe
    let top_y = match kind {
        UnitKind::Sammler => 0.7,
        UnitKind::Panzer => 0.6,
        UnitKind::Werfer => 0.9,
    };
    let child = commands
        .spawn((
            Mesh3d(a.turret.clone()),
            MeshMaterial3d(a.accent_mat[t].clone()),
            Transform::from_xyz(0.0, top_y, -0.3),
        ))
        .id();
    commands.entity(id).add_child(child);
    id
}
