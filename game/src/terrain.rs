//! Prozedurales Gelände: Höhenfunktion, Gelände-Mesh und kachelbare Detailtexturen.
//! Alles wird beim Start im Code erzeugt, es werden keine Bilddateien geladen.

use crate::components::*;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat};

/// Rohstofffelder: (Position, Art, Menge). Symmetrisch für beide Seiten.
pub const FIELDS: [(Vec3, ResKind, u32); 7] = [
    (Vec3::new(-28.0, 0.0, 50.0), ResKind::Ferrit, 1500),
    (Vec3::new(-50.0, 0.0, 26.0), ResKind::Plasma, 1000),
    (Vec3::new(-15.0, 0.0, 15.0), ResKind::Ferrit, 2500),
    (Vec3::new(28.0, 0.0, -50.0), ResKind::Ferrit, 1500),
    (Vec3::new(50.0, 0.0, -26.0), ResKind::Plasma, 1000),
    (Vec3::new(15.0, 0.0, -15.0), ResKind::Ferrit, 2500),
    (Vec3::new(0.0, 0.0, 0.0), ResKind::Plasma, 3000),
];

/// Halbe Kantenlänge des sichtbaren Geländes (größer als das Spielfeld: Berge am Rand)
pub const TERRAIN_HALF: f32 = MAP_HALF + 75.0;

// ---------------------------------------------------------------- Rauschen

fn hash(ix: i32, iz: i32, seed: u32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(374_761_393)
        ^ (iz as u32).wrapping_mul(668_265_263)
        ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h & 0x00ff_ffff) as f32 / 16_777_215.0
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    smooth(((x - e0) / (e1 - e0)).clamp(0.0, 1.0))
}

/// Wertrauschen in [0, 1]. Mit `period` wiederholt es sich (für kachelbare Texturen).
fn value_noise(x: f32, z: f32, seed: u32, period: Option<i32>) -> f32 {
    let x0 = x.floor();
    let z0 = z.floor();
    let fx = smooth(x - x0);
    let fz = smooth(z - z0);
    let (ix, iz) = (x0 as i32, z0 as i32);
    let w = |i: i32| match period {
        Some(p) => i.rem_euclid(p),
        None => i,
    };
    let a = hash(w(ix), w(iz), seed);
    let b = hash(w(ix + 1), w(iz), seed);
    let c = hash(w(ix), w(iz + 1), seed);
    let d = hash(w(ix + 1), w(iz + 1), seed);
    let top = a + (b - a) * fx;
    let bot = c + (d - c) * fx;
    top + (bot - top) * fz
}

/// Fraktales Rauschen in etwa [-1, 1]
pub fn fbm(x: f32, z: f32, seed: u32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += (value_noise(x * freq, z * freq, seed + o * 17, None) * 2.0 - 1.0) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.03;
    }
    sum / norm
}

fn fbm_periodic(x: f32, z: f32, seed: u32, octaves: u32, period: i32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1i32, 0.0);
    for o in 0..octaves {
        let f = freq as f32;
        sum += (value_noise(x * f, z * f, seed + o * 31, Some(period * freq)) * 2.0 - 1.0) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2;
    }
    sum / norm
}

// ---------------------------------------------------------------- Höhe

/// Geländehöhe an einer Stelle. Spielfeld sanft gewellt, Basen und Felder flach, Berge am Rand.
pub fn height(x: f32, z: f32) -> f32 {
    let rolling = fbm(x * 0.03, z * 0.03, 1, 4) * 1.5 + fbm(x * 0.11, z * 0.11, 7, 2) * 0.3;
    let mut flat = 1.0;
    for hq in [PLAYER_HQ_POS, ENEMY_HQ_POS] {
        flat *= smoothstep(11.0, 20.0, Vec2::new(x - hq.x, z - hq.z).length());
    }
    for (p, _, _) in FIELDS {
        flat *= smoothstep(3.0, 8.0, Vec2::new(x - p.x, z - p.z).length());
    }
    let edge = x.abs().max(z.abs());
    let ridge = 1.0 - (fbm(x * 0.045, z * 0.045, 3, 5)).abs();
    let mountains = smoothstep(MAP_HALF - 8.0, MAP_HALF + 32.0, edge) * (5.0 + ridge * ridge * 26.0);
    rolling * flat + mountains
}

pub fn normal(x: f32, z: f32) -> Vec3 {
    let e = 0.5;
    Vec3::new(
        height(x - e, z) - height(x + e, z),
        2.0 * e,
        height(x, z - e) - height(x, z + e),
    )
    .normalize()
}

/// Punkt auf Geländehöhe setzen
pub fn on_ground(p: Vec3) -> Vec3 {
    Vec3::new(p.x, height(p.x, p.z), p.z)
}

// ---------------------------------------------------------------- Mesh

fn lin(r: f32, g: f32, b: f32) -> Vec3 {
    let c = Color::srgb(r, g, b).to_linear();
    Vec3::new(c.red, c.green, c.blue)
}

pub fn build_mesh() -> Mesh {
    let step = 1.0;
    let n = (2.0 * TERRAIN_HALF / step) as usize + 1;
    let mut positions = Vec::with_capacity(n * n);
    let mut normals = Vec::with_capacity(n * n);
    let mut uvs = Vec::with_capacity(n * n);
    let mut colors = Vec::with_capacity(n * n);

    let dust = lin(0.50, 0.46, 0.39);
    let soil = lin(0.36, 0.32, 0.27);
    let scrub = lin(0.36, 0.37, 0.28);
    let rock = lin(0.42, 0.41, 0.39);
    let peak = lin(0.62, 0.61, 0.59);

    for iz in 0..n {
        for ix in 0..n {
            let x = -TERRAIN_HALF + ix as f32 * step;
            let z = -TERRAIN_HALF + iz as f32 * step;
            let h = height(x, z);
            let nrm = normal(x, z);
            positions.push([x, h, z]);
            normals.push(nrm.to_array());
            uvs.push([x / 14.0, z / 14.0]);

            // Farbe aus Rauschen, Steigung und Höhe
            let patch = fbm(x * 0.06, z * 0.06, 11, 4) * 0.5 + 0.5;
            let fine = fbm(x * 0.35, z * 0.35, 23, 3) * 0.5 + 0.5;
            let mut c = dust.lerp(soil, smoothstep(0.45, 0.75, patch));
            c = c.lerp(scrub, smoothstep(0.55, 0.85, fine) * 0.45 * (1.0 - smoothstep(2.0, 6.0, h)));
            let slope = 1.0 - nrm.y;
            c = c.lerp(rock, smoothstep(0.12, 0.35, slope));
            c = c.lerp(peak, smoothstep(18.0, 28.0, h) * 0.8);
            // Fahrspuren-Dunkelung rund um die Basen
            let mut wear = 0.0f32;
            for hq in [PLAYER_HQ_POS, ENEMY_HQ_POS] {
                let d = Vec2::new(x - hq.x, z - hq.z).length();
                wear = wear.max(1.0 - smoothstep(6.0, 16.0, d));
            }
            c *= 1.0 - wear * 0.25 * fine;
            colors.push([c.x, c.y, c.z, 1.0]);
        }
    }

    let mut indices = Vec::with_capacity((n - 1) * (n - 1) * 6);
    for iz in 0..n - 1 {
        for ix in 0..n - 1 {
            let i = (iz * n + ix) as u32;
            let r = i + 1;
            let d = i + n as u32;
            let dr = d + 1;
            indices.extend_from_slice(&[i, d, r, r, d, dr]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32(indices));
    if let Err(e) = mesh.generate_tangents() {
        warn!("Tangenten für das Gelände fehlgeschlagen: {e:?}");
    }
    mesh
}

// ---------------------------------------------------------------- Texturen

const TEX: usize = 256;
const PERIOD: i32 = 16;

/// Höhe der Detailstruktur (kachelbar)
fn detail_height(u: f32, v: f32) -> f32 {
    let large = fbm_periodic(u, v, 5, 4, PERIOD);
    // feine Kiesel / Körnung
    let grit = fbm_periodic(u * 4.0, v * 4.0, 9, 2, PERIOD * 4);
    large * 0.85 + grit * 0.15
}

/// Bild mit Mipmaps erzeugen (Box-Filter), damit die Textur in der Ferne nicht flimmert
fn image_with_mips(base: Vec<[f32; 4]>, size: usize, format: TextureFormat, srgb_encode: bool) -> Image {
    let mut levels = vec![base];
    let mut s = size;
    while s > 1 {
        let prev = levels.last().unwrap();
        let ns = s / 2;
        let mut next = vec![[0.0f32; 4]; ns * ns];
        for y in 0..ns {
            for x in 0..ns {
                let mut acc = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = prev[(y * 2 + dy) * s + x * 2 + dx];
                    for k in 0..4 {
                        acc[k] += p[k] * 0.25;
                    }
                }
                next[y * ns + x] = acc;
            }
        }
        levels.push(next);
        s = ns;
    }
    let mip_count = levels.len() as u32;
    let mut data = Vec::new();
    for level in &levels {
        for p in level {
            for (k, v) in p.iter().enumerate() {
                let v = if srgb_encode && k < 3 { v.clamp(0.0, 1.0).powf(1.0 / 2.2) } else { *v };
                data.push((v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
        }
    }
    let mut image = Image::new_uninit(
        Extent3d { width: size as u32, height: size as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = mip_count;
    image.data = Some(data);
    let mut sampler = ImageSamplerDescriptor::linear();
    sampler.set_address_mode(ImageAddressMode::Repeat);
    sampler.set_anisotropic_filter(8);
    image.sampler = ImageSampler::Descriptor(sampler);
    image
}

/// (Farb-Detailtextur, Normal-Map)
pub fn build_textures() -> (Image, Image) {
    let mut heights = vec![0.0f32; TEX * TEX];
    for y in 0..TEX {
        for x in 0..TEX {
            let u = x as f32 / TEX as f32 * PERIOD as f32;
            let v = y as f32 / TEX as f32 * PERIOD as f32;
            heights[y * TEX + x] = detail_height(u, v);
        }
    }
    let at = |x: isize, y: isize| -> f32 {
        let xi = x.rem_euclid(TEX as isize) as usize;
        let yi = y.rem_euclid(TEX as isize) as usize;
        heights[yi * TEX + xi]
    };
    let mut albedo = Vec::with_capacity(TEX * TEX);
    let mut normals = Vec::with_capacity(TEX * TEX);
    let strength = 2.2;
    for y in 0..TEX as isize {
        for x in 0..TEX as isize {
            let h = at(x, y);
            // helle Kuppen, dunkle Mulden
            let a = (0.86 + h * 0.14).clamp(0.6, 1.0);
            albedo.push([a, a * 0.98, a * 0.95, 1.0]);
            let dx = (at(x + 1, y) - at(x - 1, y)) * strength;
            let dy = (at(x, y + 1) - at(x, y - 1)) * strength;
            let n = Vec3::new(-dx, -dy, 1.0).normalize();
            normals.push([n.x * 0.5 + 0.5, n.y * 0.5 + 0.5, n.z * 0.5 + 0.5, 1.0]);
        }
    }
    (
        image_with_mips(albedo, TEX, TextureFormat::Rgba8UnormSrgb, true),
        image_with_mips(normals, TEX, TextureFormat::Rgba8Unorm, false),
    )
}

/// Felsbrocken: verbeulte Ikosphäre
pub fn rock_mesh(seed: u32) -> Mesh {
    let mut mesh = Sphere::new(1.0).mesh().ico(3).expect("Ikosphäre");
    if let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
        for p in pos.iter_mut() {
            let v = Vec3::from_array(*p);
            let n = fbm(v.x * 1.3 + v.y * 0.7 + seed as f32 * 3.1, v.z * 1.3 - v.y * 0.9, seed, 4);
            let mut q = v * (1.0 + n * 0.45);
            q.y *= 0.65;
            if q.y < -0.25 {
                q.y = -0.25 + (q.y + 0.25) * 0.3;
            }
            *p = q.to_array();
        }
    }
    mesh.compute_smooth_normals();
    mesh
}
