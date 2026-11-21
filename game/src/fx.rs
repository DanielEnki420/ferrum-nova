//! Effekte: Mündungsfeuer, Einschläge, Explosionen, Rauch und Brandspuren.

use crate::components::*;
use crate::terrain;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

#[derive(Message, Clone, Copy)]
pub enum FxEvent {
    Muzzle(Vec3),
    Impact { pos: Vec3, rocket: bool },
    Death { pos: Vec3, big: bool },
    Puff { pos: Vec3, dark: bool },
}

#[derive(Resource)]
pub struct FxKit {
    sphere: Handle<Mesh>,
    disc: Handle<Mesh>,
    flash: Handle<StandardMaterial>,
}

impl FxKit {
    pub fn new(meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> Self {
        Self {
            sphere: meshes.add(Sphere::new(0.5).mesh().ico(2).unwrap()),
            disc: meshes.add(Cylinder::new(0.5, 0.02).mesh().resolution(24)),
            flash: mats.add(StandardMaterial {
                base_color: Color::BLACK,
                emissive: LinearRgba::rgb(40.0, 26.0, 10.0),
                unlit: true,
                ..default()
            }),
        }
    }
}

#[derive(Clone, Copy)]
enum FxKind {
    Flash { size: f32 },
    Fireball { size: f32 },
    Smoke { size: f32, rise: f32, alpha: f32 },
    Scorch,
    Light { intensity: f32 },
}

#[derive(Component)]
pub struct Fx {
    age: f32,
    life: f32,
    kind: FxKind,
}

fn fx(kind: FxKind, life: f32) -> Fx {
    Fx { age: 0.0, life, kind }
}

pub fn spawn_effects(
    mut commands: Commands,
    mut events: MessageReader<FxEvent>,
    kit: Res<FxKit>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut rng: ResMut<Rng>,
) {
    for ev in events.read() {
        match *ev {
            FxEvent::Muzzle(pos) => {
                commands.spawn((
                    Mesh3d(kit.sphere.clone()),
                    MeshMaterial3d(kit.flash.clone()),
                    Transform::from_translation(pos).with_scale(Vec3::splat(0.6)),
                    NotShadowCaster,
                    fx(FxKind::Flash { size: 0.6 }, 0.07),
                ));
            }
            FxEvent::Impact { pos, rocket } => {
                let size = if rocket { 1.8 } else { 0.9 };
                fireball(&mut commands, &kit, &mut mats, pos, size, 0.35);
                for _ in 0..if rocket { 3 } else { 1 } {
                    let off = Vec3::new(rng.range(-0.5, 0.5), 0.2, rng.range(-0.5, 0.5));
                    smoke(&mut commands, &kit, &mut mats, pos + off, size * 0.9, 1.6, 0.45, true);
                }
                light(&mut commands, pos + Vec3::Y, if rocket { 1_500_000.0 } else { 500_000.0 }, 0.18);
            }
            FxEvent::Death { pos, big } => {
                let size = if big { 9.0 } else { 3.2 };
                fireball(&mut commands, &kit, &mut mats, pos + Vec3::Y * size * 0.25, size, 0.7);
                for i in 0..if big { 14 } else { 6 } {
                    let s = size * 0.5;
                    let off = Vec3::new(rng.range(-s, s), rng.range(0.3, 1.2), rng.range(-s, s));
                    smoke(
                        &mut commands,
                        &kit,
                        &mut mats,
                        pos + off,
                        size * rng.range(0.5, 0.9),
                        rng.range(1.2, 2.6),
                        0.7,
                        true,
                    );
                    if i % 3 == 0 {
                        fireball(&mut commands, &kit, &mut mats, pos + off, size * 0.5, rng.range(0.3, 0.6));
                    }
                }
                light(&mut commands, pos + Vec3::Y * 2.0, if big { 12_000_000.0 } else { 4_000_000.0 }, 0.6);
                scorch(&mut commands, &kit, &mut mats, pos, if big { 9.0 } else { 3.2 });
            }
            FxEvent::Puff { pos, dark } => {
                let size = rng.range(0.5, 0.9);
                smoke(&mut commands, &kit, &mut mats, pos, size, rng.range(1.0, 1.6), if dark { 0.55 } else { 0.35 }, dark);
            }
        }
    }
}

fn fireball(c: &mut Commands, kit: &FxKit, mats: &mut Assets<StandardMaterial>, pos: Vec3, size: f32, life: f32) {
    let m = mats.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.6, 0.2, 0.9),
        emissive: LinearRgba::rgb(30.0, 10.0, 2.5),
        alpha_mode: AlphaMode::Add,
        unlit: true,
        ..default()
    });
    c.spawn((
        Mesh3d(kit.sphere.clone()),
        MeshMaterial3d(m),
        Transform::from_translation(pos).with_scale(Vec3::splat(size * 0.3)),
        NotShadowCaster,
        NotShadowReceiver,
        fx(FxKind::Fireball { size }, life),
    ));
}

#[allow(clippy::too_many_arguments)]
fn smoke(
    c: &mut Commands,
    kit: &FxKit,
    mats: &mut Assets<StandardMaterial>,
    pos: Vec3,
    size: f32,
    life: f32,
    alpha: f32,
    dark: bool,
) {
    let g = if dark { 0.09 } else { 0.55 };
    let m = mats.add(StandardMaterial {
        base_color: Color::srgba(g, g * 0.95, g * 0.9, alpha),
        perceptual_roughness: 1.0,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    c.spawn((
        Mesh3d(kit.sphere.clone()),
        MeshMaterial3d(m),
        Transform::from_translation(pos).with_scale(Vec3::splat(size * 0.4)),
        NotShadowCaster,
        NotShadowReceiver,
        fx(FxKind::Smoke { size, rise: 1.2 + size * 0.4, alpha }, life),
    ));
}

fn scorch(c: &mut Commands, kit: &FxKit, mats: &mut Assets<StandardMaterial>, pos: Vec3, size: f32) {
    let m = mats.add(StandardMaterial {
        base_color: Color::srgba(0.02, 0.018, 0.015, 0.8),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let p = terrain::on_ground(pos);
    let n = terrain::normal(p.x, p.z);
    c.spawn((
        Mesh3d(kit.disc.clone()),
        MeshMaterial3d(m),
        Transform::from_translation(p + n * 0.06)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, n))
            .with_scale(Vec3::new(size, 1.0, size)),
        NotShadowCaster,
        NotShadowReceiver,
        fx(FxKind::Scorch, 45.0),
    ));
}

fn light(c: &mut Commands, pos: Vec3, intensity: f32, life: f32) {
    c.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.6, 0.3),
            intensity,
            range: 18.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_translation(pos),
        fx(FxKind::Light { intensity }, life),
    ));
}

pub fn update_fx(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(
        Entity,
        &mut Fx,
        &mut Transform,
        Option<&MeshMaterial3d<StandardMaterial>>,
        Option<&mut PointLight>,
    )>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    for (e, mut f, mut t, mat, light) in &mut q {
        f.age += dt;
        let k = (f.age / f.life).min(1.0);
        if k >= 1.0 {
            commands.entity(e).try_despawn();
            continue;
        }
        match f.kind {
            FxKind::Flash { size } => t.scale = Vec3::splat(size * (1.0 - k)),
            FxKind::Fireball { size } => {
                t.scale = Vec3::splat(size * (0.3 + 0.7 * (1.0 - (1.0 - k).powi(3))));
                if let Some(mut m) = mat.and_then(|m| mats.get_mut(&m.0)) {
                    let fade = (1.0 - k).powi(2);
                    m.emissive = LinearRgba::rgb(30.0 * fade, 10.0 * fade * fade, 2.5 * fade * fade);
                }
            }
            FxKind::Smoke { size, rise, alpha } => {
                t.translation.y += rise * dt * (1.0 - k * 0.6);
                t.scale = Vec3::splat(size * (0.4 + 0.8 * k));
                if let Some(mut m) = mat.and_then(|m| mats.get_mut(&m.0)) {
                    m.base_color.set_alpha(alpha * (1.0 - k) * (k * 6.0).min(1.0));
                }
            }
            FxKind::Scorch => {
                if k > 0.7
                    && let Some(mut m) = mat.and_then(|m| mats.get_mut(&m.0)) {
                        m.base_color.set_alpha(0.8 * (1.0 - (k - 0.7) / 0.3));
                    }
            }
            FxKind::Light { intensity } => {
                if let Some(mut l) = light {
                    l.intensity = intensity * (1.0 - k).powi(2);
                }
            }
        }
    }
}

/// Stark beschädigte Fahrzeuge und Gebäude qualmen
pub fn damage_smoke(
    time: Res<Time>,
    mut timer: Local<f32>,
    mut rng: ResMut<Rng>,
    q: Query<(&Transform, &Health, Has<Building>)>,
    mut w: MessageWriter<FxEvent>,
) {
    *timer -= time.delta_secs();
    if *timer > 0.0 {
        return;
    }
    *timer = 0.25;
    for (t, h, building) in &q {
        let frac = h.hp / h.max;
        if frac < 0.45 && rng.next_f32() < 0.9 - frac {
            let up = if building { 4.0 } else { 1.2 };
            let spread = if building { 2.5 } else { 0.3 };
            let off = Vec3::new(rng.range(-spread, spread), up, rng.range(-spread, spread));
            w.write(FxEvent::Puff { pos: t.translation + off, dark: true });
        }
    }
}
