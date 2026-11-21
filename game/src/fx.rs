//! Effekte: Partikelsystem mit texturierten Billboards.
//!
//! - Rauch, Staub, Feuer, Funken und Druckwellen sind Flächen, die zur Kamera zeigen,
//!   mit prozedural erzeugten, verrauschten Texturen.
//! - Das Ausblenden läuft über einen kleinen Satz gemeinsamer Materialien (Deckkraft-Stufen),
//!   damit gleichartige Partikel zusammen gezeichnet werden können.
//! - Trümmer fliegen ballistisch, prallen vom Gelände ab und bleiben liegen.
//! - Zerstörte Fahrzeuge hinterlassen ein ausgebranntes Wrack, das eine Weile brennt und qualmt.

use crate::components::*;
use crate::input::MainCamera;
use crate::models::ModelKit;
use crate::terrain;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::f32::consts::TAU;

/// Ein zerstörtes Fahrzeug, das als Wrack liegen bleibt
#[derive(Clone, Copy)]
pub struct WreckInfo {
    pub kind: UnitKind,
    pub team: Team,
    pub rot: Quat,
}

#[derive(Message, Clone, Copy)]
pub enum FxEvent {
    /// Kanonenschuss: Mündung und Schussrichtung
    Muzzle { pos: Vec3, dir: Vec3 },
    /// Raketenstart mit Rückstrahl
    Launch { pos: Vec3, dir: Vec3 },
    /// Rauchspur einer Rakete
    Trail(Vec3),
    Impact { pos: Vec3, rocket: bool },
    Death { pos: Vec3, big: bool, wreck: Option<WreckInfo> },
    Puff { pos: Vec3, dark: bool },
    /// Staubwolke beim Baubeginn und bei Fertigstellung eines Gebäudes
    BuildDust { pos: Vec3, radius: f32 },
}

/// Aussehen eines Partikels (Index in die Materialtabelle)
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    SmokeDark = 0,
    SmokeLight = 1,
    Dust = 2,
    Fire = 3,
    Spark = 4,
    Shock = 5,
}
const LEVELS: usize = 10;

#[derive(Resource)]
pub struct FxKit {
    quad: Handle<Mesh>,
    ground: Handle<Mesh>,
    chunk: Handle<Mesh>,
    looks: Vec<[Handle<StandardMaterial>; LEVELS]>,
    scorch: [Handle<StandardMaterial>; LEVELS],
    debris_rock: Handle<StandardMaterial>,
    debris_metal: Handle<StandardMaterial>,
    wreck: Handle<StandardMaterial>,
}

// ---------------------------------------------------------------- Texturen

fn texture(size: u32, f: impl Fn(f32, f32) -> [f32; 4]) -> Image {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let u = (x as f32 + 0.5) / size as f32;
            let v = (y as f32 + 0.5) / size as f32;
            let c = f(u, v);
            for (k, ch) in c.iter().enumerate() {
                // Farbkanäle sRGB-kodieren, Alpha linear
                let val = if k < 3 { ch.clamp(0.0, 1.0).powf(1.0 / 2.2) } else { ch.clamp(0.0, 1.0) };
                data.push((val * 255.0 + 0.5) as u8);
            }
        }
    }
    let mut img = Image::new(
        Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
    img
}

/// Weicher Übergang, der auch mit vertauschten Kanten funktioniert (dann fallend)
fn ss(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Abstand zur Mitte (0 Mitte, 1 Rand) und Rauschen (0..1)
fn polar(u: f32, v: f32, seed: u32, freq: f32) -> (f32, f32) {
    let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * 2.0;
    let n = terrain::fbm(u * freq, v * freq, seed, 4) * 0.5 + 0.5;
    (d, n)
}

fn smoke_tex() -> Image {
    texture(96, |u, v| {
        let (d, n) = polar(u, v, 5, 5.0);
        let a = ss(1.0, 0.15, d + (n - 0.5) * 0.6) * (0.65 + 0.35 * n);
        let g = 0.75 + 0.25 * n;
        [g, g, g, a]
    })
}

fn fire_tex() -> Image {
    texture(96, |u, v| {
        let (d, n) = polar(u, v, 9, 6.0);
        // zerfranste Flammenzungen statt einer runden Scheibe
        let (_, n2) = polar(u * 1.7, v * 0.6, 21, 9.0);
        let a = ss(0.95, 0.1, d + (n - 0.5) * 1.1) * (0.3 + 0.7 * n2).powf(1.3);
        let hot = (1.0 - d).clamp(0.0, 1.0);
        [1.0, 0.45 + 0.5 * hot, 0.15 + 0.6 * hot * hot, a]
    })
}

fn spark_tex() -> Image {
    texture(32, |u, v| {
        let (d, _) = polar(u, v, 1, 1.0);
        let a = (1.0 - d).clamp(0.0, 1.0).powi(3);
        [1.0, 0.9, 0.7, a]
    })
}

fn shock_tex() -> Image {
    texture(128, |u, v| {
        let (d, n) = polar(u, v, 13, 8.0);
        let a = (-((d - 0.82) / 0.09).powi(2)).exp() * (0.5 + 0.5 * n);
        [1.0, 0.95, 0.85, a]
    })
}

fn scorch_tex() -> Image {
    texture(128, |u, v| {
        let (d, n) = polar(u, v, 17, 5.0);
        let a = ss(1.0, 0.25, d + (n - 0.5) * 0.7) * 0.95;
        let c = 0.02 + 0.05 * n;
        [c * 1.1, c, c * 0.9, a]
    })
}

impl FxKit {
    pub fn new(meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, images: &mut Assets<Image>) -> Self {
        let smoke = images.add(smoke_tex());
        let fire = images.add(fire_tex());
        let spark = images.add(spark_tex());
        let shock = images.add(shock_tex());
        let scorch = images.add(scorch_tex());

        let blend = |mats: &mut Assets<StandardMaterial>, tex: &Handle<Image>, r: f32, g: f32, b: f32, peak: f32| {
            std::array::from_fn(|i| {
                let a = peak * i as f32 / (LEVELS - 1) as f32;
                mats.add(StandardMaterial {
                    base_color: Color::srgba(r, g, b, a),
                    base_color_texture: Some(tex.clone()),
                    alpha_mode: AlphaMode::Blend,
                    perceptual_roughness: 1.0,
                    reflectance: 0.0,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                })
            })
        };
        let add = |mats: &mut Assets<StandardMaterial>, tex: &Handle<Image>, c: LinearRgba| {
            std::array::from_fn(|i| {
                let k = i as f32 / (LEVELS - 1) as f32;
                mats.add(StandardMaterial {
                    base_color: Color::LinearRgba(LinearRgba::new(c.red * k, c.green * k, c.blue * k, k)),
                    base_color_texture: Some(tex.clone()),
                    alpha_mode: AlphaMode::Add,
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                })
            })
        };
        let looks = vec![
            blend(mats, &smoke, 0.07, 0.065, 0.06, 0.9),
            blend(mats, &smoke, 0.88, 0.86, 0.82, 0.8),
            blend(mats, &smoke, 0.58, 0.49, 0.37, 0.7),
            add(mats, &fire, LinearRgba::rgb(3.6, 1.25, 0.32)),
            add(mats, &spark, LinearRgba::rgb(14.0, 8.0, 3.0)),
            add(mats, &shock, LinearRgba::rgb(3.0, 2.3, 1.6)),
        ];
        let scorch = std::array::from_fn(|i| {
            mats.add(StandardMaterial {
                base_color: Color::srgba(1.0, 1.0, 1.0, i as f32 / (LEVELS - 1) as f32),
                base_color_texture: Some(scorch.clone()),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 1.0,
                reflectance: 0.0,
                ..default()
            })
        });

        let mut chunk = Sphere::new(0.5).mesh().ico(0).expect("Trümmer");
        chunk.scale_by(Vec3::new(1.0, 0.6, 0.8));
        chunk.duplicate_vertices();
        chunk.compute_flat_normals();

        Self {
            quad: meshes.add(Rectangle::new(1.0, 1.0)),
            ground: meshes.add(Plane3d::default().mesh().size(1.0, 1.0)),
            chunk: meshes.add(chunk),
            looks,
            scorch,
            debris_rock: mats.add(StandardMaterial {
                base_color: Color::srgb(0.30, 0.27, 0.23),
                perceptual_roughness: 0.95,
                ..default()
            }),
            debris_metal: mats.add(StandardMaterial {
                base_color: Color::srgb(0.12, 0.11, 0.10),
                metallic: 0.6,
                perceptual_roughness: 0.6,
                ..default()
            }),
            wreck: mats.add(StandardMaterial {
                // Vertexfarben werden mit diesem Grundton multipliziert: verkohlt
                base_color: Color::srgb(0.16, 0.14, 0.13),
                perceptual_roughness: 0.95,
                metallic: 0.1,
                ..default()
            }),
        }
    }
}

// ---------------------------------------------------------------- Komponenten

#[derive(Component)]
pub struct Particle {
    look: Look,
    vel: Vec3,
    drag: f32,
    age: f32,
    life: f32,
    size0: f32,
    size1: f32,
    angle: f32,
    spin: f32,
    peak: f32,
    fade_in: f32,
    /// Funken werden entlang der Bewegung gestreckt
    streak: bool,
    level: usize,
}

/// Erdbrocken in der Fontäne fallen zurück
#[derive(Component)]
pub struct Heavy;

#[derive(Component)]
pub struct Debris {
    vel: Vec3,
    axis: Vec3,
    spin: f32,
    age: f32,
    life: f32,
    resting: bool,
}

#[derive(Component)]
pub struct Decal {
    age: f32,
    life: f32,
    peak: f32,
    level: usize,
    grow: f32,
    size: f32,
}

#[derive(Component)]
pub struct Flash {
    intensity: f32,
    age: f32,
    life: f32,
}

/// Brennendes Wrack: Feuer und Rauch, später versinkt es im Boden
#[derive(Component)]
pub struct Wreck {
    age: f32,
    fire: f32,
    smoke: f32,
    timer: f32,
    big: bool,
}

// ---------------------------------------------------------------- Erzeugung

struct Spawner<'a, 'w, 's> {
    c: &'a mut Commands<'w, 's>,
    kit: &'a FxKit,
    rng: &'a mut Rng,
}

impl Spawner<'_, '_, '_> {
    #[allow(clippy::too_many_arguments)]
    fn particle(&mut self, look: Look, pos: Vec3, vel: Vec3, life: f32, size0: f32, size1: f32, peak: f32) -> EntityCommands<'_> {
        let p = Particle {
            look,
            vel,
            drag: 1.5,
            age: 0.0,
            life,
            size0,
            size1,
            angle: self.rng.range(0.0, TAU),
            spin: self.rng.range(-0.8, 0.8),
            peak,
            fade_in: if matches!(look, Look::Fire | Look::Spark) { 0.02 } else { 0.12 },
            streak: look == Look::Spark,
            level: usize::MAX,
        };
        self.c.spawn((
            Mesh3d(self.kit.quad.clone()),
            MeshMaterial3d(self.kit.looks[look as usize][0].clone()),
            Transform::from_translation(pos).with_scale(Vec3::splat(size0)),
            NotShadowCaster,
            NotShadowReceiver,
            p,
        ))
    }

    fn rand_dir(&mut self, up: f32) -> Vec3 {
        let a = self.rng.range(0.0, TAU);
        let y = self.rng.range(up, 1.0);
        let r = (1.0 - y * y).max(0.0).sqrt();
        Vec3::new(a.cos() * r, y, a.sin() * r)
    }

    fn sparks(&mut self, pos: Vec3, n: usize, speed: f32) {
        for _ in 0..n {
            let v = self.rand_dir(0.1) * self.rng.range(speed * 0.5, speed);
            let life = self.rng.range(0.35, 0.8);
            let size = self.rng.range(0.06, 0.12);
            self.particle(Look::Spark, pos, v, life, size, size * 0.5, 1.0);
        }
    }

    fn fire_burst(&mut self, pos: Vec3, n: usize, size: f32, life: f32, rise: f32) {
        for _ in 0..n {
            let off = self.rand_dir(-0.3) * size * 0.35;
            let v = off * 2.5 + Vec3::Y * rise * self.rng.range(0.5, 1.0);
            let l = life * self.rng.range(0.6, 1.1);
            let s = size * self.rng.range(0.5, 0.9);
            self.particle(Look::Fire, pos + off, v, l, s * 0.5, s * 1.3, 1.0);
        }
    }

    fn smoke(&mut self, look: Look, pos: Vec3, n: usize, size: f32, life: f32, rise: f32, spread: f32, peak: f32) {
        for _ in 0..n {
            let off = Vec3::new(self.rng.range(-spread, spread), self.rng.range(0.0, spread * 0.5), self.rng.range(-spread, spread));
            let v = off * 0.6 + Vec3::Y * rise * self.rng.range(0.6, 1.2) + Vec3::new(0.25, 0.0, 0.12);
            let l = life * self.rng.range(0.7, 1.2);
            let s = size * self.rng.range(0.7, 1.1);
            self.particle(look, pos + off, v, l, s * 0.4, s * 1.6, peak);
        }
    }

    fn dust_ring(&mut self, pos: Vec3, n: usize, radius: f32, speed: f32) {
        let g = terrain::on_ground(pos);
        for i in 0..n {
            let a = TAU * i as f32 / n as f32 + self.rng.range(-0.2, 0.2);
            let d = Vec3::new(a.cos(), 0.0, a.sin());
            let s = self.rng.range(0.8, 1.2);
            self.particle(Look::Dust, g + d * radius * 0.3 + Vec3::Y * 0.3, d * speed * s + Vec3::Y * 0.4, 1.6 * s, radius * 0.4, radius * 1.1, 0.8);
        }
    }

    fn debris(&mut self, pos: Vec3, n: usize, speed: f32, metal: bool, size: f32) {
        for _ in 0..n {
            let v = self.rand_dir(0.35) * self.rng.range(speed * 0.4, speed);
            let s = size * self.rng.range(0.4, 1.0);
            let axis = self.rand_dir(-1.0);
            let mat = if metal && self.rng.next_f32() < 0.7 { &self.kit.debris_metal } else { &self.kit.debris_rock };
            self.c.spawn((
                Mesh3d(self.kit.chunk.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(pos).with_scale(Vec3::splat(s)),
                NotShadowReceiver,
                Debris { vel: v, axis, spin: self.rng.range(4.0, 12.0), age: 0.0, life: self.rng.range(10.0, 18.0), resting: false },
            ));
        }
    }

    fn scorch(&mut self, pos: Vec3, size: f32, life: f32) {
        let g = terrain::on_ground(pos);
        let n = terrain::normal(g.x, g.z);
        self.c.spawn((
            Mesh3d(self.kit.ground.clone()),
            MeshMaterial3d(self.kit.scorch[LEVELS - 1].clone()),
            Transform::from_translation(g + n * 0.05)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, n) * Quat::from_rotation_y(self.rng.range(0.0, TAU)))
                .with_scale(Vec3::splat(size)),
            NotShadowCaster,
            Decal { age: 0.0, life, peak: 1.0, level: LEVELS - 1, grow: 0.0, size },
        ));
    }

    fn shockwave(&mut self, pos: Vec3, size: f32) {
        let g = terrain::on_ground(pos);
        self.c.spawn((
            Mesh3d(self.kit.ground.clone()),
            MeshMaterial3d(self.kit.looks[Look::Shock as usize][LEVELS - 1].clone()),
            Transform::from_translation(g + Vec3::Y * 0.15).with_scale(Vec3::splat(size * 0.2)),
            NotShadowCaster,
            NotShadowReceiver,
            Decal { age: 0.0, life: 0.45, peak: 1.0, level: usize::MAX, grow: size, size: size * 0.2 },
        ));
    }

    fn flash(&mut self, pos: Vec3, intensity: f32, life: f32, range: f32) {
        self.c.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.62, 0.32),
                intensity,
                range,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(pos),
            Flash { intensity, age: 0.0, life },
        ));
    }
}

pub fn spawn_effects(
    mut commands: Commands,
    mut events: MessageReader<FxEvent>,
    kit: Res<FxKit>,
    models: Res<ModelKit>,
    mut rng: ResMut<Rng>,
) {
    let mut s = Spawner { c: &mut commands, kit: &kit, rng: &mut rng };
    for ev in events.read() {
        match *ev {
            FxEvent::Muzzle { pos, dir } => {
                // Mündungsfeuer als kurzer Kegel aus Feuerflächen
                for (k, size) in [(0.0, 0.9), (0.35, 0.7), (0.7, 0.45)] {
                    let life = s.rng.range(0.09, 0.14);
                    s.particle(Look::Fire, pos + dir * k, dir * 3.0, life, size, size * 1.2, 1.0);
                }
                for side in [-1.0f32, 1.0] {
                    let lat = dir.cross(Vec3::Y).normalize_or_zero() * side;
                    s.particle(Look::Fire, pos + lat * 0.15, lat * 6.0 + dir * 2.0, 0.07, 0.4, 0.6, 0.8);
                }
                s.smoke(Look::SmokeLight, pos + dir * 0.5, 4, 0.9, 1.6, 0.4, 0.25, 0.8);
                s.flash(pos + dir * 0.5, 350_000.0, 0.08, 10.0);
                if pos.y - terrain::height(pos.x, pos.z) < 1.6 {
                    s.dust_ring(pos, 5, 1.0, 1.6);
                }
            }
            FxEvent::Launch { pos, dir } => {
                s.fire_burst(pos, 3, 0.6, 0.15, 0.5);
                for _ in 0..7 {
                    let v = -dir * s.rng.range(2.0, 4.5) + Vec3::new(s.rng.range(-0.8, 0.8), s.rng.range(0.0, 0.6), s.rng.range(-0.8, 0.8));
                    let life = s.rng.range(1.2, 2.0);
                    s.particle(Look::SmokeLight, pos - dir * 0.8, v, life, 0.6, 2.2, 0.85);
                }
                s.dust_ring(pos, 6, 1.4, 2.0);
                s.flash(pos, 500_000.0, 0.12, 10.0);
            }
            FxEvent::Trail(pos) => {
                s.particle(Look::Fire, pos, Vec3::ZERO, 0.08, 0.35, 0.2, 1.0);
                let life = s.rng.range(1.0, 1.6);
                s.particle(Look::SmokeLight, pos, Vec3::Y * 0.3, life, 0.25, 1.1, 0.55);
            }
            FxEvent::Impact { pos, rocket } => {
                let size = if rocket { 2.2 } else { 1.1 };
                s.fire_burst(pos, if rocket { 7 } else { 3 }, size, 0.3, 1.5);
                s.sparks(pos, if rocket { 14 } else { 7 }, if rocket { 11.0 } else { 8.0 });
                let ground = pos.y - terrain::height(pos.x, pos.z) < 1.0;
                if ground {
                    // Erdfontäne und Brocken
                    for _ in 0..if rocket { 8 } else { 4 } {
                        let v = s.rand_dir(0.6) * s.rng.range(2.0, 6.0);
                        let life = s.rng.range(0.8, 1.5);
                        s.particle(Look::Dust, pos, v, life, size * 0.3, size * 1.1, 0.85).insert(Heavy);
                    }
                    s.debris(pos + Vec3::Y * 0.2, if rocket { 6 } else { 2 }, 7.0, false, 0.2);
                    s.scorch(pos, if rocket { 2.4 } else { 1.0 }, 30.0);
                }
                s.smoke(Look::SmokeDark, pos, if rocket { 4 } else { 2 }, size, 2.2, 0.8, 0.3, 0.6);
                s.flash(pos + Vec3::Y, if rocket { 1_600_000.0 } else { 500_000.0 }, 0.18, 16.0);
            }
            FxEvent::Death { pos, big, wreck } => {
                let size = if big { 7.0 } else { 2.8 };
                let core = pos + Vec3::Y * size * 0.25;
                // Feuerball aus vielen Lagen, die aufquellen und aufsteigen
                s.fire_burst(core, if big { 16 } else { 9 }, size * 0.75, 0.9, size * 0.6);
                s.fire_burst(core, if big { 6 } else { 4 }, size * 0.5, 0.35, 0.5);
                s.shockwave(pos, size * 2.4);
                s.sparks(core, if big { 40 } else { 18 }, if big { 20.0 } else { 13.0 });
                s.debris(core, if big { 24 } else { 9 }, if big { 16.0 } else { 11.0 }, true, if big { 0.55 } else { 0.3 });
                s.dust_ring(pos, if big { 16 } else { 10 }, size * 0.9, size * 0.9);
                // Rauchpilz: dunkle Säule, die lange steht
                s.smoke(Look::SmokeDark, core, if big { 22 } else { 9 }, size * 0.9, if big { 7.0 } else { 4.5 }, size * 0.35, size * 0.35, 0.85);
                s.scorch(pos, size * 1.6, 50.0);
                s.flash(pos + Vec3::Y * 2.0, if big { 14_000_000.0 } else { 4_500_000.0 }, 0.7, if big { 40.0 } else { 22.0 });
                if let Some(w) = wreck {
                    spawn_wreck(s.c, &kit, &models, s.rng, pos, w);
                }
            }
            FxEvent::BuildDust { pos, radius } => {
                s.dust_ring(pos, 14, radius * 1.1, radius * 0.9);
            }
            FxEvent::Puff { pos, dark } => {
                let look = if dark { Look::SmokeDark } else { Look::SmokeLight };
                s.smoke(look, pos, 1, 0.9, 2.0, 0.9, 0.1, if dark { 0.7 } else { 0.5 });
            }
        }
    }
}

fn spawn_wreck(c: &mut Commands, kit: &FxKit, models: &ModelKit, rng: &mut Rng, pos: Vec3, w: WreckInfo) {
    let vm = match w.kind {
        UnitKind::Sammler => &models.vehicles[0][w.team.0 as usize],
        UnitKind::Panzer => &models.vehicles[1][w.team.0 as usize],
        UnitKind::Werfer => &models.vehicles[2][w.team.0 as usize],
    };
    let ground = terrain::on_ground(pos);
    let tilt = Quat::from_rotation_x(rng.range(-0.08, 0.08)) * Quat::from_rotation_z(rng.range(-0.1, 0.1));
    let root = c
        .spawn((
            Transform::from_translation(ground - Vec3::Y * 0.08).with_rotation(w.rot * tilt),
            Visibility::default(),
            Wreck { age: 0.0, fire: rng.range(8.0, 14.0), smoke: rng.range(22.0, 30.0), timer: 0.0, big: w.kind != UnitKind::Sammler },
        ))
        .id();
    c.entity(root).with_child((Mesh3d(vm.hull.clone()), MeshMaterial3d(kit.wreck.clone())));
    if let Some(t) = &vm.turret {
        // Turm verdreht oder abgesprengt
        let blown = rng.next_f32() < 0.35;
        let tf = if blown {
            let a = rng.range(0.0, TAU);
            Transform::from_xyz(a.cos() * 2.4, 0.15, a.sin() * 2.4)
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, TAU)) * Quat::from_rotation_z(rng.range(-0.5, 0.5)))
        } else {
            let y = if w.kind == UnitKind::Werfer { 1.05 } else { 0.86 };
            Transform::from_xyz(rng.range(-0.1, 0.1), y, 0.1).with_rotation(Quat::from_rotation_y(rng.range(-1.2, 1.2)) * Quat::from_rotation_x(rng.range(-0.15, 0.05)))
        };
        c.entity(root).with_child((Mesh3d(t.clone()), MeshMaterial3d(kit.wreck.clone()), tf));
    }
}

// ---------------------------------------------------------------- Aktualisierung

pub fn update_fx(
    mut commands: Commands,
    time: Res<Time>,
    kit: Res<FxKit>,
    cam: Query<&GlobalTransform, With<MainCamera>>,
    mut particles: Query<(Entity, &mut Particle, &mut Transform, &mut MeshMaterial3d<StandardMaterial>, Has<Heavy>), (Without<Debris>, Without<Decal>)>,
    mut debris: Query<(Entity, &mut Debris, &mut Transform), (Without<Particle>, Without<Decal>)>,
    mut decals: Query<(Entity, &mut Decal, &mut Transform, &mut MeshMaterial3d<StandardMaterial>), (Without<Particle>, Without<Debris>)>,
    mut flashes: Query<(Entity, &mut Flash, &mut PointLight)>,
) {
    let dt = time.delta_secs().min(0.05);
    let Ok(cam) = cam.single() else { return };
    let cam_pos = cam.translation();
    let cam_rot = cam.compute_transform().rotation;

    for (e, mut p, mut t, mut mat, heavy) in &mut particles {
        p.age += dt;
        let k = p.age / p.life;
        if k >= 1.0 {
            commands.entity(e).try_despawn();
            continue;
        }
        let g = if p.look == Look::Spark || heavy { 9.0 } else { 0.0 };
        p.vel.y -= g * dt;
        let drag = (1.0 - p.drag * dt).max(0.0);
        p.vel *= drag;
        t.translation += p.vel * dt;
        // Rauch steigt weiter, Feuer wird langsamer
        if matches!(p.look, Look::SmokeDark | Look::SmokeLight) {
            t.translation.y += 0.35 * dt;
        }
        p.angle += p.spin * dt;
        let size = p.size0 + (p.size1 - p.size0) * (1.0 - (1.0 - k).powi(2));

        if p.streak && p.vel.length_squared() > 0.01 {
            let up = p.vel.normalize();
            let to_cam = (cam_pos - t.translation).normalize_or_zero();
            let right = up.cross(to_cam).normalize_or_zero();
            let fwd = right.cross(up);
            t.rotation = Quat::from_mat3(&Mat3::from_cols(right, up, fwd));
            t.scale = Vec3::new(size, size * (1.0 + p.vel.length() * 0.12) * 4.0, 1.0);
        } else {
            t.rotation = cam_rot * Quat::from_rotation_z(p.angle);
            t.scale = Vec3::splat(size);
        }

        let alpha = p.peak * (k / p.fade_in).min(1.0) * (1.0 - k).powf(1.3);
        let level = ((alpha * (LEVELS - 1) as f32).round() as usize).min(LEVELS - 1);
        if level != p.level {
            p.level = level;
            mat.0 = kit.looks[p.look as usize][level].clone();
        }
    }

    for (e, mut d, mut t) in &mut debris {
        d.age += dt;
        if d.age > d.life {
            commands.entity(e).try_despawn();
            continue;
        }
        if !d.resting {
            d.vel.y -= 14.0 * dt;
            t.translation += d.vel * dt;
            t.rotate(Quat::from_axis_angle(d.axis, d.spin * dt));
            let floor = terrain::height(t.translation.x, t.translation.z) + t.scale.y * 0.2;
            if t.translation.y < floor {
                t.translation.y = floor;
                if d.vel.y.abs() < 1.5 {
                    d.resting = true;
                } else {
                    d.vel = Vec3::new(d.vel.x * 0.45, -d.vel.y * 0.3, d.vel.z * 0.45);
                    d.spin *= 0.5;
                }
            }
        } else if d.life - d.age < 2.0 {
            // zum Schluss im Boden versinken
            t.translation.y -= 0.15 * dt;
        }
    }

    for (e, mut d, mut t, mut mat) in &mut decals {
        d.age += dt;
        let k = d.age / d.life;
        if k >= 1.0 {
            commands.entity(e).try_despawn();
            continue;
        }
        let (alpha, set) = if d.grow > 0.0 {
            // Druckwelle: schnell wachsen und verblassen
            let s = d.size + (d.grow - d.size) * (1.0 - (1.0 - k).powi(3));
            t.scale = Vec3::splat(s);
            ((1.0 - k).powi(2) * d.peak, &kit.looks[Look::Shock as usize])
        } else {
            (if k > 0.75 { d.peak * (1.0 - (k - 0.75) / 0.25) } else { d.peak }, &kit.scorch)
        };
        let level = ((alpha * (LEVELS - 1) as f32).round() as usize).min(LEVELS - 1);
        if level != d.level {
            d.level = level;
            mat.0 = set[level].clone();
        }
    }

    for (e, mut f, mut l) in &mut flashes {
        f.age += dt;
        if f.age >= f.life {
            commands.entity(e).try_despawn();
            continue;
        }
        let k = f.age / f.life;
        // leichtes Flackern
        l.intensity = f.intensity * (1.0 - k).powi(2) * (0.85 + 0.15 * (f.age * 60.0).sin());
    }
}

/// Wracks brennen, qualmen und versinken schließlich
pub fn update_wrecks(
    mut commands: Commands,
    time: Res<Time>,
    kit: Res<FxKit>,
    mut rng: ResMut<Rng>,
    mut q: Query<(Entity, &mut Wreck, &mut Transform)>,
) {
    let dt = time.delta_secs();
    let mut s = Spawner { c: &mut commands, kit: &kit, rng: &mut rng };
    for (e, mut w, mut t) in &mut q {
        w.age += dt;
        w.timer -= dt;
        if w.age > 55.0 {
            t.translation.y -= 0.3 * dt;
            if w.age > 62.0 {
                s.c.entity(e).try_despawn();
            }
            continue;
        }
        if w.timer > 0.0 || w.age > w.smoke {
            continue;
        }
        w.timer = 0.07;
        let top = t.translation + Vec3::Y * if w.big { 1.0 } else { 1.2 };
        let jitter = Vec3::new(s.rng.range(-0.5, 0.5), 0.0, s.rng.range(-0.5, 0.5));
        if w.age < w.fire {
            let strength = 1.0 - w.age / w.fire;
            let life = s.rng.range(0.3, 0.55);
            let side = Vec3::new(s.rng.range(-0.4, 0.4), 0.0, s.rng.range(-0.4, 0.4));
            s.particle(Look::Fire, top + jitter * 1.3, Vec3::Y * 2.6 + side, life, 0.7 * (0.5 + strength), 0.25, 1.0);
        }
        let fade = 1.0 - w.age / w.smoke;
        if s.rng.next_f32() < 0.45 {
            let life = s.rng.range(3.0, 4.5);
            s.particle(Look::SmokeDark, top + jitter + Vec3::Y * 0.6, Vec3::new(0.3, 1.2, 0.15), life, 0.7, 2.8, 0.9 * fade);
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
