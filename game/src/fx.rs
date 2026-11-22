//! Effekte: Partikelsystem mit texturierten Billboards.
//!
//! - Rauch, Staub, Feuer, Funken und Druckwellen sind Flächen, die zur Kamera zeigen,
//!   mit prozedural erzeugten, verrauschten Texturen.
//! - Das Ausblenden läuft über einen kleinen Satz gemeinsamer Materialien (Deckkraft-Stufen),
//!   damit gleichartige Partikel zusammen gezeichnet werden können.
//! - Trümmer fliegen ballistisch, prallen vom Gelände ab und bleiben liegen.
//! - Zerstörte Fahrzeuge hinterlassen ein ausgebranntes Wrack, das eine Weile brennt und qualmt.
//! - Jede Waffenklasse hat eigenes Mündungsfeuer und eigene Einschläge (Boden, Panzerung, Beton).
//! - Zerstörte Gebäude stürzen in mehreren Explosionen ein und hinterlassen einen Trümmerhaufen.
//! - Umgebung: Schornsteine dampfen und rauchen, auf Baustellen sprühen Schweißfunken,
//!   über Plasmaquellen steigen leuchtende Teilchen auf.

use crate::components::*;
use crate::input::MainCamera;
use crate::models::ModelKit;
use crate::terrain;
use crate::vehicles::{lin, Geo};
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
    /// Kanonenschuss: Mündung, Schussrichtung und Waffenklasse
    Muzzle { pos: Vec3, dir: Vec3, caliber: Caliber },
    /// Raketenstart mit Rückstrahl
    Launch { pos: Vec3, dir: Vec3 },
    /// Rauchspur einer Rakete
    Trail(Vec3),
    Impact { pos: Vec3, caliber: Caliber, surface: Surface },
    Death { pos: Vec3, big: bool, wreck: Option<WreckInfo> },
    Puff { pos: Vec3, dark: bool },
    /// Staubwolke beim Baubeginn und bei Fertigstellung eines Gebäudes
    BuildDust { pos: Vec3, radius: f32 },
    /// Staubfahne hinter fahrenden Fahrzeugen
    Dust { pos: Vec3, size: f32 },
    /// Fahrspur im Boden (beide Ketten bzw. Radspuren in einem Abdruck)
    Track { pos: Vec3, rot: Quat, width: f32 },
    /// Kleine Flamme an schwer beschädigten Fahrzeugen
    Flame(Vec3),
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
    /// leuchtende Plasmateilchen
    Plasma = 6,
    /// Lichtbogen beim Schweißen
    Arc = 7,
}
const LEVELS: usize = 10;

#[derive(Resource)]
pub struct FxKit {
    quad: Handle<Mesh>,
    ground: Handle<Mesh>,
    chunk: Handle<Mesh>,
    looks: Vec<[Handle<StandardMaterial>; LEVELS]>,
    scorch: [Handle<StandardMaterial>; LEVELS],
    tracks: [Handle<StandardMaterial>; LEVELS],
    debris_rock: Handle<StandardMaterial>,
    debris_metal: Handle<StandardMaterial>,
    wreck: Handle<StandardMaterial>,
    casing: Handle<Mesh>,
    brass: Handle<StandardMaterial>,
    /// Trümmerhaufen in mehreren Varianten (Radius 1)
    rubble: Vec<Handle<Mesh>>,
    rubble_mat: Handle<StandardMaterial>,
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

/// Zwei Spurbänder mit Stollenmuster (Kette oder Reifenprofil)
fn track_tex() -> Image {
    texture(64, |u, v| {
        // Spurmitten bei ±1/2,6 der Abdruckbreite (Breite = Spurweite · 2,6)
        let band = |c: f32| {
            let d = (u - c).abs();
            ss(0.12, 0.085, d)
        };
        let lanes = band(0.115) + band(0.885);
        let cleat = 0.55 + 0.45 * ss(0.2, 0.5, ((v * 9.0).fract() - 0.5).abs() * 2.0);
        let n = terrain::fbm(u * 6.0, v * 6.0, 29, 3) * 0.5 + 0.5;
        let a = lanes * cleat * (0.6 + 0.4 * n);
        [0.12, 0.10, 0.08, a * 0.55]
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

/// Trümmerhaufen mit Radius 1: Betonbrocken zu einem Hügel, Wandreste, verbogene Träger
fn rubble_mesh(seed: u64) -> Mesh {
    let mut g = Geo::default();
    let mut r = Rng(seed);
    let concrete = [
        lin(Color::srgb(0.47, 0.45, 0.41)),
        lin(Color::srgb(0.37, 0.35, 0.32)),
        lin(Color::srgb(0.26, 0.24, 0.22)),
        lin(Color::srgb(0.11, 0.10, 0.09)),
    ];
    let steel = [lin(Color::srgb(0.30, 0.17, 0.10)), lin(Color::srgb(0.16, 0.16, 0.17))];
    // flacher, unregelmäßiger Sockel
    let plan: Vec<Vec2> = (0..9)
        .map(|i| {
            let a = TAU * i as f32 / 9.0;
            Vec2::new(a.cos(), a.sin()) * r.range(0.8, 1.0)
        })
        .collect();
    g.extrude_y(&plan, -0.05, 0.06, 0.95, 0.0, concrete[2]);
    for _ in 0..52 {
        let a = r.range(0.0, TAU);
        let d = r.next_f32().sqrt() * 0.9;
        let mound = (1.0 - d) * 0.5;
        let size = Vec3::new(r.range(0.08, 0.3), r.range(0.05, 0.18), r.range(0.08, 0.3));
        let rot = Quat::from_euler(EulerRot::XYZ, r.range(-0.6, 0.6), r.range(0.0, TAU), r.range(-0.6, 0.6));
        let col = concrete[(r.next_f32() * 4.0) as usize % 4];
        g.cuboid(Vec3::new(a.cos() * d, r.range(0.0, mound) + size.y * 0.3, a.sin() * d), size, rot, col);
    }
    // stehengebliebene Wandreste
    for _ in 0..3 {
        let a = r.range(0.0, TAU);
        let d = r.range(0.4, 0.8);
        let size = Vec3::new(r.range(0.45, 0.8), r.range(0.35, 0.7), 0.08);
        let rot = Quat::from_rotation_y(a + r.range(-0.4, 0.4)) * Quat::from_rotation_x(r.range(-0.35, 0.35));
        g.cuboid(Vec3::new(a.cos() * d, size.y * 0.4, a.sin() * d), size, rot, concrete[0]);
    }
    // verbogene Träger und ein Rohr, schräg aus dem Haufen
    for i in 0..7 {
        let a = r.range(0.0, TAU);
        let d = r.range(0.1, 0.7);
        let len = r.range(0.5, 0.9);
        let rot = Quat::from_rotation_y(r.range(0.0, TAU)) * Quat::from_rotation_x(r.range(0.3, 1.0));
        g.cuboid(Vec3::new(a.cos() * d, 0.2 + len * 0.2, a.sin() * d), Vec3::new(0.045, 0.045, len), rot, steel[i % 2]);
    }
    let axis = Vec3::new(r.range(-1.0, 1.0), 0.6, r.range(-1.0, 1.0));
    g.cyl(Vec3::new(r.range(-0.4, 0.4), 0.3, r.range(-0.4, 0.4)), 0.06, 0.8, axis, 8, steel[1]);
    g.weather(lin(Color::srgb(0.58, 0.49, 0.37)), 0.25, 0.45);
    g.build().expect("Trümmer")
}

impl FxKit {
    pub fn new(meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, images: &mut Assets<Image>) -> Self {
        let smoke = images.add(smoke_tex());
        let fire = images.add(fire_tex());
        let spark = images.add(spark_tex());
        let shock = images.add(shock_tex());
        let scorch = images.add(scorch_tex());
        let track = images.add(track_tex());

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
            add(mats, &spark, LinearRgba::rgb(5.5, 1.4, 8.0)),
            add(mats, &spark, LinearRgba::rgb(4.0, 6.5, 9.0)),
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

        let tracks = std::array::from_fn(|i| {
            mats.add(StandardMaterial {
                base_color: Color::srgba(1.0, 1.0, 1.0, i as f32 / (LEVELS - 1) as f32),
                base_color_texture: Some(track.clone()),
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
            casing: meshes.add(Cylinder::new(0.045, 0.2).mesh().resolution(6).build()),
            brass: mats.add(StandardMaterial {
                base_color: Color::srgb(0.75, 0.55, 0.25),
                metallic: 0.9,
                perceptual_roughness: 0.35,
                ..default()
            }),
            rubble: (0..3).map(|i| meshes.add(rubble_mesh(0xA5_1234 + i * 7919))).collect(),
            rubble_mat: mats.add(StandardMaterial { perceptual_roughness: 0.95, ..default() }),
            quad: meshes.add(Rectangle::new(1.0, 1.0)),
            ground: meshes.add(Plane3d::default().mesh().size(1.0, 1.0)),
            chunk: meshes.add(chunk),
            looks,
            scorch,
            tracks,
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
    track: bool,
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
    /// Größe der Brandstelle (1 bei Fahrzeugen, größer bei Trümmerhaufen)
    size: f32,
    /// so lange bleibt es liegen, bevor es versinkt
    stay: f32,
}

/// Ein zerstörtes Gebäude sinkt unter Explosionen in sich zusammen
#[derive(Component)]
pub struct Collapse {
    age: f32,
    timer: f32,
    radius: f32,
    depth: f32,
    base: Vec3,
    rot: Quat,
    axis: Vec3,
}

const COLLAPSE_TIME: f32 = 2.8;

impl Collapse {
    pub fn new(t: &Transform, radius: f32) -> Self {
        let p = t.translation;
        Self {
            age: 0.0,
            timer: 0.0,
            radius,
            depth: radius * 1.5,
            base: p,
            rot: t.rotation,
            axis: Vec3::new((p.x * 1.7).sin(), 0.0, (p.z * 1.3).cos()).normalize_or(Vec3::X),
        }
    }
}

// ---------------------------------------------------------------- Erzeugung

struct Spawner<'a, 'w, 's> {
    c: &'a mut Commands<'w, 's>,
    kit: &'a FxKit,
    rng: &'a mut Rng,
    /// So viele Lichtblitze dürfen noch dazukommen (WebGL verkraftet nur wenige Punktlichter)
    lights: usize,
}

/// Höchstzahl gleichzeitiger Lichtblitze. Jedes Punktlicht kostet in jedem Bildpunkt Rechenzeit;
/// bei zu vielen (große Mauer: 100 Baustellen schweißen gleichzeitig) setzt der Grafiktreiber
/// den WebGL-Kontext zurück und das Bild wird schwarz.
const MAX_FLASHES: usize = 10;
/// Höchstens so viele Baustellen schweißen je Funkenrunde (alle 0,1 s)
const MAX_WELDS: usize = 4;
/// Staub von Baustellen je Bild: Mauern entstehen zu Dutzenden gleichzeitig
const MAX_BUILD_DUST: usize = 3;
const MAX_PUFFS: usize = 6;

fn light_budget(flashes: &Query<(), With<Flash>>) -> usize {
    MAX_FLASHES.saturating_sub(flashes.iter().count())
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
            fade_in: if matches!(look, Look::Fire | Look::Spark | Look::Arc) { 0.02 } else { 0.12 },
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
            Decal { age: 0.0, life, peak: 1.0, level: LEVELS - 1, grow: 0.0, size, track: false },
        ));
    }

    fn track(&mut self, pos: Vec3, rot: Quat, width: f32) {
        let g = terrain::on_ground(pos);
        let n = terrain::normal(g.x, g.z);
        self.c.spawn((
            Mesh3d(self.kit.ground.clone()),
            MeshMaterial3d(self.kit.tracks[LEVELS - 1].clone()),
            Transform::from_translation(g + n * 0.03)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, n) * rot)
                .with_scale(Vec3::new(width, 1.0, 1.45)),
            NotShadowCaster,
            Decal { age: 0.0, life: 16.0, peak: 1.0, level: LEVELS - 1, grow: 0.0, size: 1.0, track: true },
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
            Decal { age: 0.0, life: 0.45, peak: 1.0, level: usize::MAX, grow: size, size: size * 0.2, track: false },
        ));
    }

    fn flash(&mut self, pos: Vec3, intensity: f32, life: f32, range: f32) {
        self.flash_col(pos, intensity, life, range, Color::srgb(1.0, 0.62, 0.32));
    }

    fn flash_col(&mut self, pos: Vec3, intensity: f32, life: f32, range: f32, color: Color) {
        if self.lights == 0 {
            return;
        }
        self.lights -= 1;
        self.c.spawn((
            PointLight {
                color,
                intensity,
                range,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(pos),
            Flash { intensity, age: 0.0, life },
        ));
    }

    /// Ausgeworfene Messinghülse, die vom Boden abprallt
    fn casing(&mut self, pos: Vec3, dir: Vec3, size: f32) {
        let side = dir.cross(Vec3::Y).normalize_or(Vec3::X);
        let v = side * self.rng.range(2.5, 4.0) + Vec3::Y * self.rng.range(2.0, 3.5) - dir * 0.5;
        let axis = self.rand_dir(-1.0);
        self.c.spawn((
            Mesh3d(self.kit.casing.clone()),
            MeshMaterial3d(self.kit.brass.clone()),
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_arc(Vec3::Y, side)).with_scale(Vec3::splat(size)),
            NotShadowCaster,
            NotShadowReceiver,
            Debris { vel: v, axis, spin: self.rng.range(14.0, 24.0), age: 0.0, life: self.rng.range(6.0, 9.0), resting: false },
        ));
    }

    /// Rauchring quer zur Schussrichtung (Mündungsbremse schwerer Rohre)
    #[allow(clippy::too_many_arguments)]
    fn smoke_ring(&mut self, pos: Vec3, dir: Vec3, n: usize, radius: f32, speed: f32, size: f32, life: f32) {
        let u = dir.cross(Vec3::Y).normalize_or(Vec3::X);
        let v = dir.cross(u).normalize_or(Vec3::Y);
        for i in 0..n {
            let a = TAU * i as f32 / n as f32 + self.rng.range(-0.15, 0.15);
            let d = u * a.cos() + v * a.sin();
            let l = life * self.rng.range(0.8, 1.1);
            self.particle(Look::SmokeLight, pos + d * radius, d * speed + dir * speed * 0.4, l, size * 0.6, size * 1.8, 0.7);
        }
    }

    /// Mündungsfeuer je Waffenklasse
    fn muzzle(&mut self, pos: Vec3, dir: Vec3, caliber: Caliber) {
        let k = caliber.scale();
        let lat = dir.cross(Vec3::Y).normalize_or_zero();
        match caliber {
            Caliber::Light => {
                // kurzes, helles Züngeln der Maschinenkanone
                for (o, size) in [(0.0, 0.45), (0.25, 0.3)] {
                    let life = self.rng.range(0.05, 0.08);
                    self.particle(Look::Fire, pos + dir * o, dir * 2.0, life, size, size * 1.2, 1.0);
                }
                if self.rng.next_f32() < 0.5 {
                    self.particle(Look::SmokeLight, pos + dir * 0.3, dir * 1.2 + Vec3::Y * 0.3, 0.8, 0.3, 0.9, 0.5);
                }
                self.flash(pos, 120_000.0, 0.05, 6.0);
                self.casing(pos - dir * 1.1, dir, 0.6);
            }
            Caliber::Medium | Caliber::Heavy | Caliber::Rocket => {
                let heavy = caliber == Caliber::Heavy;
                let layers: &[(f32, f32)] = if heavy { &[(0.0, 1.3), (0.45, 1.05), (0.95, 0.75), (1.4, 0.45)] } else { &[(0.0, 0.9), (0.35, 0.7), (0.7, 0.45)] };
                let stretch = if heavy { 1.3 } else { 1.0 };
                for &(o, size) in layers {
                    let life = self.rng.range(0.09, 0.14) * stretch;
                    self.particle(Look::Fire, pos + dir * o, dir * 3.0, life, size, size * 1.2, 1.0);
                }
                // Mündungsbremse: seitliche Feuerstrahlen
                for side in [-1.0f32, 1.0] {
                    let l = lat * side;
                    self.particle(Look::Fire, pos + l * 0.15, l * 6.0 * k + dir * 2.0, 0.07 * stretch, 0.4 * k, 0.6 * k, 0.8);
                    if heavy {
                        self.particle(Look::SmokeLight, pos + l * 0.4, l * 3.0 + Vec3::Y * 0.4, 1.6, 0.4, 1.6, 0.6);
                    }
                }
                self.smoke(Look::SmokeLight, pos + dir * 0.5, if heavy { 7 } else { 4 }, 0.9 * k, 1.6 * stretch, 0.4, 0.25 * k, 0.8);
                self.flash(pos + dir * 0.5, 350_000.0 * k * k, 0.08 * stretch, 10.0 * k);
                if pos.y - terrain::height(pos.x, pos.z) < 1.6 {
                    self.dust_ring(pos, (5.0 * k) as usize, k, 1.6 * k);
                }
                self.casing(pos - dir * if heavy { 2.6 } else { 1.9 }, dir, k);
            }
            Caliber::Artillery => {
                // langer Feuerstrahl, Rauchring und Druckwelle am Boden
                for (o, size) in [(0.0, 1.6), (0.6, 1.4), (1.3, 1.1), (2.0, 0.8), (2.6, 0.5)] {
                    let life = self.rng.range(0.12, 0.18);
                    self.particle(Look::Fire, pos + dir * o, dir * 5.0, life, size, size * 1.3, 1.0);
                }
                for side in [-1.0f32, 1.0] {
                    let l = lat * side;
                    self.particle(Look::Fire, pos + l * 0.2, l * 10.0 + dir * 3.0, 0.12, 0.9, 1.4, 0.9);
                }
                self.smoke_ring(pos + dir * 0.6, dir, 12, 0.5, 2.6, 1.0, 2.4);
                self.smoke(Look::SmokeLight, pos + dir * 1.2, 8, 1.6, 3.2, 0.5, 0.8, 0.75);
                let ground = pos - dir * 2.2;
                self.dust_ring(ground, 14, 3.0, 3.5);
                self.shockwave(ground, 7.0);
                self.flash(pos, 2_500_000.0, 0.16, 22.0);
            }
        }
    }

    /// Einschlag je Waffenklasse und getroffener Oberfläche
    fn impact(&mut self, pos: Vec3, caliber: Caliber, surface: Surface) {
        let k = caliber.scale();
        let g = terrain::on_ground(pos);
        match surface {
            Surface::Armor => {
                // Stahl: Funkenregen und Querschläger
                self.sparks(pos, (6.0 + 10.0 * k) as usize, 9.0 + 3.0 * k);
                for _ in 0..(2.0 + 2.0 * k) as usize {
                    let v = self.rand_dir(0.45) * self.rng.range(14.0, 22.0);
                    let life = self.rng.range(0.25, 0.45);
                    self.particle(Look::Spark, pos, v, life, 0.1, 0.05, 1.0);
                }
                self.fire_burst(pos, (2.0 + 2.0 * k) as usize, 0.7 * k, 0.2, 1.0);
                if k >= 1.0 {
                    self.debris(pos, (k * 2.0) as usize, 6.0, true, 0.12 * k);
                }
                self.smoke(Look::SmokeDark, pos, 1 + k as usize, 0.8 * k, 1.8, 0.8, 0.2 * k, 0.6);
                self.flash(pos + Vec3::Y * 0.5, 400_000.0 * k, 0.12, 8.0 + 4.0 * k);
            }
            Surface::Concrete => {
                // Beton: Splitter und grauer Staub
                self.sparks(pos, (3.0 + 5.0 * k) as usize, 8.0);
                for _ in 0..(3.0 + 3.0 * k) as usize {
                    let v = self.rand_dir(-0.2) * self.rng.range(1.5, 4.0) * k.sqrt();
                    let life = self.rng.range(1.4, 2.2);
                    self.particle(Look::SmokeLight, pos, v, life, 0.5 * k, 1.6 * k, 0.55);
                }
                for _ in 0..3 {
                    let v = self.rand_dir(0.3) * self.rng.range(3.0, 6.0);
                    self.particle(Look::Dust, pos, v, 0.8, 0.2 * k, 0.5 * k, 0.8).insert(Heavy);
                }
                self.debris(pos, (1.0 + 2.0 * k) as usize, 6.0, false, 0.14 * k);
                self.fire_burst(pos, (1.0 + 2.0 * k) as usize, 0.6 * k, 0.2, 1.0);
                self.flash(pos + Vec3::Y * 0.5, 350_000.0 * k, 0.12, 8.0 + 4.0 * k);
            }
            Surface::Ground => {
                self.fire_burst(g + Vec3::Y * 0.3 * k, (2.0 + 3.0 * k) as usize, 0.9 * k, 0.3, 1.5);
                self.sparks(g, (3.0 + 4.0 * k) as usize, 8.0);
                // Erdfontäne
                for _ in 0..(3.0 + 4.0 * k) as usize {
                    let v = self.rand_dir(0.7) * self.rng.range(2.5, 6.0) * k.sqrt();
                    let life = self.rng.range(0.9, 1.6);
                    self.particle(Look::Dust, g, v, life, 0.3 * k, 1.1 * k, 0.85).insert(Heavy);
                }
                self.debris(g + Vec3::Y * 0.2, (1.0 + 2.5 * k) as usize, 6.0 + k, false, 0.18 * k.sqrt());
                self.scorch(g, k, 30.0 + 10.0 * k);
                self.smoke(Look::SmokeDark, g, 1 + (1.5 * k) as usize, k, 2.2, 0.8, 0.3 * k, 0.6);
                if k > 2.0 {
                    // Granate oder Rakete: Druckwelle und Staubring
                    self.shockwave(g, 3.0 * k);
                    self.dust_ring(g, 10, 1.2 * k, 1.4 * k);
                }
                if caliber == Caliber::Artillery {
                    // hohe Erdsäule, die zurückfällt
                    for _ in 0..7 {
                        let v = Vec3::new(self.rng.range(-1.2, 1.2), self.rng.range(7.0, 11.0), self.rng.range(-1.2, 1.2));
                        let life = self.rng.range(1.4, 1.9);
                        self.particle(Look::Dust, g, v, life, 0.6, 2.2, 0.85).insert(Heavy);
                    }
                }
                self.flash(g + Vec3::Y, 500_000.0 * k * k, 0.18, 10.0 + 4.0 * k);
            }
        }
        if caliber == Caliber::Rocket {
            self.fire_burst(pos, 4, 1.6, 0.35, 1.2);
        }
    }

    /// Lichtbogen und herabfallende Funken auf der Baustelle
    fn weld(&mut self, pos: Vec3) {
        for _ in 0..9 {
            let v = self.rand_dir(-0.2) * self.rng.range(2.0, 5.0);
            let life = self.rng.range(0.4, 0.9);
            self.particle(Look::Spark, pos, v, life, 0.09, 0.04, 1.0);
        }
        self.particle(Look::Arc, pos, Vec3::ZERO, 0.09, 0.7, 0.4, 1.0);
        self.flash_col(pos, 250_000.0, 0.07, 7.0, Color::srgb(0.6, 0.8, 1.0));
    }

    /// Dampf (hell) oder Abgas (dunkel) aus einem Schornstein
    fn vent(&mut self, pos: Vec3, steam: bool) {
        let v = Vec3::new(self.rng.range(-0.15, 0.15) + 0.3, self.rng.range(1.4, 2.0), self.rng.range(-0.15, 0.15) + 0.15);
        if steam {
            let life = self.rng.range(3.0, 4.5);
            self.particle(Look::SmokeLight, pos, v, life, 0.45, 3.4, 0.8);
        } else {
            let life = self.rng.range(4.0, 5.5);
            self.particle(Look::SmokeDark, pos, v, life, 0.5, 3.8, 0.8);
        }
    }
}

pub fn spawn_effects(
    mut commands: Commands,
    mut events: MessageReader<FxEvent>,
    kit: Res<FxKit>,
    models: Res<ModelKit>,
    mut rng: ResMut<Rng>,
    flashes: Query<(), With<Flash>>,
) {
    let mut s = Spawner { c: &mut commands, kit: &kit, rng: &mut rng, lights: light_budget(&flashes) };
    let (mut dust, mut puffs) = (0, 0);
    for ev in events.read() {
        // Baustellen-Staub begrenzen: eine lange Mauer meldet Dutzende Baustellen im selben Bild
        match ev {
            FxEvent::BuildDust { .. } => {
                dust += 1;
                if dust > MAX_BUILD_DUST {
                    continue;
                }
            }
            FxEvent::Puff { .. } => {
                puffs += 1;
                if puffs > MAX_PUFFS {
                    continue;
                }
            }
            _ => {}
        }
        match *ev {
            FxEvent::Muzzle { pos, dir, caliber } => s.muzzle(pos, dir, caliber),
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
            FxEvent::Impact { pos, caliber, surface } => s.impact(pos, caliber, surface),
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
            FxEvent::Dust { pos, size } => {
                let v = Vec3::new(s.rng.range(-0.3, 0.3), s.rng.range(0.2, 0.5), s.rng.range(-0.3, 0.3));
                let life = s.rng.range(1.0, 1.6);
                s.particle(Look::Dust, pos, v, life, size * 0.5, size * 2.2, 0.32);
            }
            FxEvent::Track { pos, rot, width } => s.track(pos, rot, width),
            FxEvent::Flame(pos) => {
                let life = s.rng.range(0.3, 0.5);
                let v = Vec3::new(s.rng.range(-0.2, 0.2), 2.2, s.rng.range(-0.2, 0.2));
                s.particle(Look::Fire, pos, v, life, 0.5, 0.15, 1.0);
            }
            FxEvent::Puff { pos, dark } => {
                let look = if dark { Look::SmokeDark } else { Look::SmokeLight };
                s.smoke(look, pos, 1, 0.9, 2.0, 0.9, 0.1, if dark { 0.7 } else { 0.5 });
            }
        }
    }
}

fn spawn_wreck(c: &mut Commands, kit: &FxKit, models: &ModelKit, rng: &mut Rng, pos: Vec3, w: WreckInfo) {
    let vm = &models.vehicles[w.kind.idx()][w.team.0 as usize];
    let ground = terrain::on_ground(pos);
    let tilt = Quat::from_rotation_x(rng.range(-0.08, 0.08)) * Quat::from_rotation_z(rng.range(-0.1, 0.1));
    let root = c
        .spawn((
            Transform::from_translation(ground - Vec3::Y * 0.08).with_rotation(w.rot * tilt),
            Visibility::default(),
            Wreck {
                age: 0.0,
                fire: rng.range(8.0, 14.0),
                smoke: rng.range(22.0, 30.0),
                timer: 0.0,
                big: w.kind != UnitKind::Sammler,
                size: 1.0,
                stay: 55.0,
            },
        ))
        .id();
    c.entity(root).with_child((Mesh3d(vm.hull.clone()), MeshMaterial3d(kit.wreck.clone())));
    for (m, p, _) in &vm.wheels {
        c.entity(root).with_child((Mesh3d(m.clone()), MeshMaterial3d(kit.wreck.clone()), Transform::from_translation(*p)));
    }
    if let Some(t) = &vm.turret {
        // Turm verdreht oder abgesprengt
        let blown = rng.next_f32() < 0.35;
        let tf = if blown {
            let a = rng.range(0.0, TAU);
            Transform::from_xyz(a.cos() * 2.4, 0.15, a.sin() * 2.4)
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, TAU)) * Quat::from_rotation_z(rng.range(-0.5, 0.5)))
        } else {
            let m = w.kind.mount().map_or((0.86, 0.1), |m| (m.y, m.z));
            Transform::from_xyz(rng.range(-0.1, 0.1), m.0, m.1).with_rotation(Quat::from_rotation_y(rng.range(-1.2, 1.2)) * Quat::from_rotation_x(rng.range(-0.15, 0.05)))
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
        if matches!(p.look, Look::SmokeDark | Look::SmokeLight | Look::Plasma) {
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
            let fade_from = if d.track { 0.4 } else { 0.75 };
            let a = if k > fade_from { d.peak * (1.0 - (k - fade_from) / (1.0 - fade_from)) } else { d.peak };
            (a, if d.track { &kit.tracks } else { &kit.scorch })
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
    flashes: Query<(), With<Flash>>,
    mut q: Query<(Entity, &mut Wreck, &mut Transform)>,
) {
    let dt = time.delta_secs();
    let mut s = Spawner { c: &mut commands, kit: &kit, rng: &mut rng, lights: light_budget(&flashes) };
    for (e, mut w, mut t) in &mut q {
        w.age += dt;
        w.timer -= dt;
        if w.age > w.stay {
            t.translation.y -= 0.3 * w.size * dt;
            if w.age > w.stay + 7.0 {
                s.c.entity(e).try_despawn();
            }
            continue;
        }
        if w.timer > 0.0 || w.age > w.smoke {
            continue;
        }
        // Trümmerhaufen brennen an mehreren Stellen, daher öfter
        w.timer = if w.size > 1.0 { 0.04 } else { 0.07 };
        let top = t.translation + Vec3::Y * if w.size > 1.0 { w.size * 0.55 } else if w.big { 1.0 } else { 1.2 };
        let jitter = Vec3::new(s.rng.range(-0.5, 0.5), 0.0, s.rng.range(-0.5, 0.5)) * w.size;
        let grow = w.size.sqrt();
        if w.age < w.fire {
            let strength = 1.0 - w.age / w.fire;
            let life = s.rng.range(0.3, 0.55);
            let side = Vec3::new(s.rng.range(-0.4, 0.4), 0.0, s.rng.range(-0.4, 0.4));
            s.particle(Look::Fire, top + jitter * 1.3, Vec3::Y * 2.6 + side, life, 0.7 * (0.5 + strength) * grow, 0.25, 1.0);
        }
        let fade = 1.0 - w.age / w.smoke;
        if s.rng.next_f32() < 0.45 {
            let life = s.rng.range(3.0, 4.5);
            s.particle(Look::SmokeDark, top + jitter + Vec3::Y * 0.6, Vec3::new(0.3, 1.2, 0.15), life, 0.7 * grow, 2.8 * grow, 0.9 * fade);
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
            if frac < 0.25 {
                w.write(FxEvent::Flame(t.translation + off * 0.8));
            }
        }
    }
}

/// Einsturz: das Gebäude sackt ab und kippt, Explosionen reißen es auf,
/// am Ende bleibt ein brennender Trümmerhaufen in einer Staubwolke
pub fn update_collapse(
    mut commands: Commands,
    time: Res<Time>,
    kit: Res<FxKit>,
    mut rng: ResMut<Rng>,
    flashes: Query<(), With<Flash>>,
    mut q: Query<(Entity, &mut Collapse, &mut Transform)>,
) {
    let dt = time.delta_secs().min(0.05);
    let mut s = Spawner { c: &mut commands, kit: &kit, rng: &mut rng, lights: light_budget(&flashes) };
    for (e, mut c, mut t) in &mut q {
        c.age += dt;
        c.timer -= dt;
        let k = (c.age / COLLAPSE_TIME).min(1.0);
        let sink = k * k;
        let shake = (c.age * 43.0).sin() * 0.06 * (1.0 - k);
        t.translation = c.base - Vec3::Y * c.depth * sink + Vec3::new(shake, 0.0, shake * 0.7);
        t.rotation = Quat::from_axis_angle(c.axis, 0.16 * sink) * c.rot;
        if c.timer <= 0.0 && k < 0.9 {
            c.timer = s.rng.range(0.15, 0.3);
            let a = s.rng.range(0.0, TAU);
            let r = s.rng.range(0.0, c.radius * 0.8);
            let h = s.rng.range(0.5, 0.5 + c.depth * (1.0 - sink) * 0.8);
            let p = c.base + Vec3::new(a.cos() * r, h, a.sin() * r);
            let metal = s.rng.next_f32() < 0.5;
            s.fire_burst(p, 4, 1.6, 0.4, 1.5);
            s.sparks(p, 8, 10.0);
            s.debris(p, 3, 9.0, metal, 0.35);
            s.smoke(Look::SmokeDark, p, 2, 2.0, 3.5, 1.2, 0.8, 0.8);
            s.flash(p, 1_200_000.0, 0.2, 18.0);
        }
        // Staub quillt am Fuß heraus
        if s.rng.next_f32() < 0.6 {
            let a = s.rng.range(0.0, TAU);
            let d = Vec3::new(a.cos(), 0.0, a.sin());
            let g = terrain::on_ground(c.base + d * c.radius);
            let life = s.rng.range(1.8, 2.6);
            s.particle(Look::Dust, g + Vec3::Y * 0.4, d * 2.5 + Vec3::Y * 0.6, life, 1.0, 3.2, 0.75);
        }
        if c.age >= COLLAPSE_TIME {
            s.c.entity(e).try_despawn();
            s.dust_ring(c.base, 18, c.radius * 1.2, c.radius);
            s.smoke(Look::Dust, c.base + Vec3::Y, 10, c.radius * 0.9, 4.0, 0.6, c.radius * 0.6, 0.7);
            let mesh = s.kit.rubble[(s.rng.next_f32() * 3.0) as usize % 3].clone();
            let ground = terrain::on_ground(c.base);
            let rot = Quat::from_rotation_y(s.rng.range(0.0, TAU));
            let (fire, smoke) = (s.rng.range(18.0, 26.0), s.rng.range(45.0, 60.0));
            s.c.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(s.kit.rubble_mat.clone()),
                Transform::from_translation(ground - Vec3::Y * 0.05).with_rotation(rot).with_scale(Vec3::new(c.radius, c.radius * 0.8, c.radius)),
                Wreck { age: 0.0, fire, smoke, timer: 0.0, big: true, size: c.radius * 0.5, stay: 110.0 },
            ));
            s.scorch(ground, c.radius * 2.6, 120.0);
        }
    }
}

/// Umgebungseffekte: Schornsteine, Schweißarbeiten auf Baustellen, Plasmaquellen
pub fn ambient_fx(
    mut commands: Commands,
    time: Res<Time>,
    kit: Res<FxKit>,
    models: Res<ModelKit>,
    mut rng: ResMut<Rng>,
    flashes: Query<(), With<Flash>>,
    mut timer: Local<f32>,
    structures: Query<(&Transform, &Structure, &Team, Option<&Construction>)>,
    nodes: Query<(&Transform, &ResourceNode)>,
) {
    *timer -= time.delta_secs();
    if *timer > 0.0 {
        return;
    }
    *timer = 0.1;
    let mut s = Spawner { c: &mut commands, kit: &kit, rng: &mut rng, lights: light_budget(&flashes) };
    let mut welds = 0;
    for (t, st, team, cons) in &structures {
        let sm = &models.structures[st.0.idx()][team.0 as usize];
        match cons {
            Some(c) => {
                // Schweißen an der schon sichtbaren Höhe des wachsenden Gebäudes
                let top = sm.height * (1.0 - (1.0 - c.progress) * (1.0 - c.progress));
                if top < 0.6 || welds >= MAX_WELDS || s.rng.next_f32() > 0.8 {
                    continue;
                }
                welds += 1;
                // am Rand, damit die Wände den Lichtbogen nicht verdecken
                let a = s.rng.range(0.0, TAU);
                let r = st.0.radius() * s.rng.range(0.75, 0.95);
                let h = s.rng.range(0.4, top);
                s.weld(t.translation + Vec3::new(a.cos() * r, h, a.sin() * r));
            }
            None => {
                for &(v, steam) in &sm.vents {
                    if s.rng.next_f32() < 0.7 {
                        s.vent(t.transform_point(v), steam);
                    }
                }
            }
        }
    }
    for (t, n) in &nodes {
        if n.kind != ResKind::Plasma || n.amount == 0 || s.rng.next_f32() > 0.6 {
            continue;
        }
        let p = t.translation + Vec3::new(s.rng.range(-2.2, 2.2), s.rng.range(0.2, 0.8), s.rng.range(-2.2, 2.2));
        let v = Vec3::new(s.rng.range(-0.1, 0.1), s.rng.range(0.5, 1.0), s.rng.range(-0.1, 0.1));
        let life = s.rng.range(1.6, 2.6);
        let size = s.rng.range(0.35, 0.55);
        s.particle(Look::Plasma, p, v, life, size, 0.1, 1.0);
    }
}
