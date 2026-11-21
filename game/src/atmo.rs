//! Atmosphäre am Boden: Leuchtschimmer um die Rohstofffelder und Staubschwaden, die mit dem Wind ziehen.

use crate::components::*;
use crate::input::{CamRig, MainCamera};
use crate::terrain;
use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Windrichtung (gleich wie im Wind-Shader und bei den Wolkenschatten)
pub fn wind_dir() -> Vec3 {
    Vec3::new(1.0, 0.0, 0.55).normalize()
}

/// Weiche, runde Graustufen-Textur: radialer Abfall, optional mit Wolkenstruktur
fn soft_blob(size: usize, cloudy: bool, seed: u32) -> Image {
    let mut data = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let u = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let v = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let r = (u * u + v * v).sqrt();
            let mut a = (1.0 - r).clamp(0.0, 1.0).powf(1.6);
            if cloudy {
                let n = terrain::fbm(u * 2.2 + seed as f32, v * 2.2, 77 + seed, 4) * 0.5 + 0.5;
                a *= terrain::smoothstep(0.25, 0.75, n);
            }
            let i = (y * size + x) * 4;
            data[i..i + 4].copy_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(
        Extent3d { width: size as u32, height: size as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Gemeinsame Materialien des Bodenschimmers (pulsieren im Takt)
#[derive(Resource)]
pub struct FieldGlow {
    pub ferrit: Handle<StandardMaterial>,
    pub plasma: Handle<StandardMaterial>,
    pub mesh: Handle<Mesh>,
}

const FERRIT_GLOW: LinearRgba = LinearRgba::rgb(0.05, 0.9, 0.6);
const PLASMA_GLOW: LinearRgba = LinearRgba::rgb(1.1, 0.25, 1.6);

pub fn setup_atmo(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>) {
    let blob = images.add(soft_blob(64, false, 0));
    let glow = |mats: &mut Assets<StandardMaterial>, c: LinearRgba| {
        mats.add(StandardMaterial {
            base_color: Color::LinearRgba(c),
            base_color_texture: Some(blob.clone()),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            fog_enabled: true,
            ..default()
        })
    };
    let glow = FieldGlow {
        ferrit: glow(&mut mats, FERRIT_GLOW),
        plasma: glow(&mut mats, PLASMA_GLOW),
        // flach auf dem Boden, 14 m Durchmesser
        mesh: meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(7.0))),
    };
    // Schimmer an jedes Feld hängen (die Felder entstehen in setup_world, das vorher läuft)
    commands.insert_resource(glow);

    // Staubschwaden
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    for i in 0..DUST_COUNT {
        let tex = images.add(soft_blob(64, true, i as u32 * 13 + 1));
        let mat = mats.add(StandardMaterial {
            base_color: DUST_COLOR.with_alpha(0.0),
            base_color_texture: Some(tex),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        commands.spawn((
            Mesh3d(quad.clone()),
            MeshMaterial3d(mat),
            Transform::default(),
            Visibility::Hidden,
            NotShadowCaster,
            NotShadowReceiver,
            // gestaffelt starten, damit nicht alle gleichzeitig auftauchen
            Dust { age: -(i as f32) * 1.7, life: 0.0, vel: Vec3::ZERO, size: Vec2::ONE, peak: 0.0, base: Vec3::ZERO },
        ));
    }
}

/// Schimmer unter die Felder legen, sobald es sie gibt
pub fn attach_field_glow(
    mut commands: Commands,
    glow: Res<FieldGlow>,
    nodes: Query<(Entity, &ResourceNode), Added<ResourceNode>>,
) {
    for (e, n) in &nodes {
        let (mat, scale) = match n.kind {
            ResKind::Ferrit => (glow.ferrit.clone(), 0.85),
            ResKind::Plasma => (glow.plasma.clone(), 1.0),
        };
        commands.entity(e).with_child((
            Mesh3d(glow.mesh.clone()),
            MeshMaterial3d(mat),
            Transform::from_xyz(0.0, 0.12, 0.0).with_scale(Vec3::splat(scale)),
            NotShadowCaster,
            NotShadowReceiver,
            crate::resources::Shrinks,
        ));
    }
}

/// Schimmer atmet langsam
pub fn pulse_field_glow(time: Res<Time>, glow: Res<FieldGlow>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let t = time.elapsed_secs();
    for (h, c, phase) in [(&glow.ferrit, FERRIT_GLOW, 0.0), (&glow.plasma, PLASMA_GLOW, 1.3)] {
        let k = 0.8 + 0.2 * (t * 1.3 + phase).sin() + 0.05 * (t * 4.1 + phase).sin();
        if let Some(mut m) = mats.get_mut(h) {
            m.base_color = Color::LinearRgba(c * k);
        }
    }
}

const DUST_COUNT: usize = 10;
const DUST_COLOR: Color = Color::srgb(0.74, 0.67, 0.56);

/// Eine Staubschwade: zieht mit dem Wind, blendet weich ein und aus
#[derive(Component)]
pub struct Dust {
    age: f32,
    life: f32,
    vel: Vec3,
    size: Vec2,
    peak: f32,
    base: Vec3,
}

pub fn update_dust(
    time: Res<Time>,
    mut rng: ResMut<Rng>,
    rig: Res<CamRig>,
    cam: Query<&Transform, (With<MainCamera>, Without<Dust>)>,
    mut q: Query<(&mut Dust, &mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    let Ok(cam_t) = cam.single() else { return };
    for (mut d, mut t, mut vis, mat) in &mut q {
        d.age += dt;
        if d.age < 0.0 {
            continue;
        }
        if d.age >= d.life {
            // neu um den Blickpunkt herum aufsteigen lassen, eher gegen den Wind versetzt
            let a = rng.range(0.0, std::f32::consts::TAU);
            let r = rng.range(8.0, 45.0);
            let p = rig.focus + Vec3::new(a.cos(), 0.0, a.sin()) * r - wind_dir() * 12.0;
            d.base = terrain::on_ground(p);
            d.age = 0.0;
            d.life = rng.range(14.0, 24.0);
            d.vel = wind_dir() * rng.range(1.5, 3.2);
            d.size = Vec2::new(rng.range(12.0, 24.0), rng.range(4.0, 7.0));
            d.peak = rng.range(0.10, 0.18);
            *vis = Visibility::Inherited;
        }
        let k = d.age / d.life;
        let alpha = d.peak * (k * std::f32::consts::PI).sin().max(0.0);
        let pos = d.base + d.vel * d.age;
        let ground = terrain::height(pos.x, pos.z);
        t.translation = Vec3::new(pos.x, ground + d.size.y * 0.35, pos.z);
        // zur Kamera gedreht, aber aufrecht
        let to_cam = (cam_t.translation - t.translation).with_y(0.0).normalize_or(Vec3::Z);
        t.rotation = Quat::from_rotation_arc(Vec3::Z, to_cam);
        t.scale = Vec3::new(d.size.x * (1.0 + k * 0.3), d.size.y * (1.0 + k * 0.2), 1.0);
        if let Some(mut m) = mats.get_mut(&mat.0) {
            m.base_color = DUST_COLOR.with_alpha(alpha);
        }
    }
}
