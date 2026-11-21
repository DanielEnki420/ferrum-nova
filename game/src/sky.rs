//! Himmel als Licht: eine prozedurale Umgebungskarte (Cubemap) aus Himmelsverlauf, Sonne und Boden.
//!
//! Die Kamera blickt immer schräg nach unten, den Himmel selbst sieht man nie. Er wirkt aber über
//! das Licht: blaues Himmelslicht von oben, warmes Bodenlicht von unten, und Spiegelungen auf Lack,
//! Glas und Kristallen. WebGL2 kann Umgebungskarten nicht auf der GPU filtern, deshalb werden
//! Streulicht (diffus) und die nach Rauheit gestaffelten Spiegelungs-Mipmaps hier auf der CPU berechnet.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::light::EnvironmentMapLight;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension};

/// Richtung zur Sonne (die Sonne steht tief im Westen, für lange Schatten)
pub fn sun_dir() -> Vec3 {
    Vec3::new(-40.0, 38.0, 30.0).normalize()
}

fn srgb(r: f32, g: f32, b: f32) -> Vec3 {
    let c = LinearRgba::from(Color::srgb(r, g, b));
    Vec3::new(c.red, c.green, c.blue)
}

/// Himmelsleuchtdichte in Richtung `d`, unscharf wie für eine Oberfläche mit Rauheit `rough` (0..1)
pub fn sky(d: Vec3, rough: f32) -> Vec3 {
    let zenith = srgb(0.30, 0.50, 0.80);
    let horizon = srgb(0.72, 0.70, 0.66);
    let ground = srgb(0.40, 0.34, 0.27) * 0.55;
    // Unschärfe: Verlauf weicher, Horizont verwaschen
    let y = d.y;
    let soft = 0.06 + rough * 0.5;
    let up = (y / soft).clamp(-1.0, 1.0) * 0.5 + 0.5;
    let sky_col = horizon.lerp(zenith, y.max(0.0).powf(0.5 + rough * 0.5));
    let mut col = ground.lerp(sky_col, up);
    // Sonne: scharfe Scheibe mit Hof, bei rauen Flächen ein breiter, schwacher Fleck
    let s = d.dot(sun_dir()).max(0.0);
    let n = 2.0 + (1.0 - rough).powi(3) * 1400.0;
    let sun = Vec3::new(1.0, 0.86, 0.66);
    col += sun * s.powf(n) * (n + 2.0) / (8.0 + n * 0.012) * 0.06;
    col += sun * s.powf(8.0) * 0.35;
    col
}

/// Würfelseite + Koordinaten (-1..1, v nach unten) -> Richtung (wgpu-Konvention)
fn face_dir(face: usize, u: f32, v: f32) -> Vec3 {
    match face {
        0 => Vec3::new(1.0, -v, -u),
        1 => Vec3::new(-1.0, -v, u),
        2 => Vec3::new(u, 1.0, v),
        3 => Vec3::new(u, -1.0, -v),
        4 => Vec3::new(u, -v, 1.0),
        _ => Vec3::new(-u, -v, -1.0),
    }
    .normalize()
}

/// f32 -> IEEE-Halbfloat (für Rgba16Float, das WebGL2 filtern kann)
fn f16_bits(x: f32) -> u16 {
    let b = x.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
    let man = b & 0x7f_ffff;
    if exp <= 0 {
        return sign;
    }
    if exp >= 31 {
        return sign | 0x7bff;
    }
    sign | ((exp as u16) << 10) | (man >> 13) as u16
}

/// Cubemap aus einer Funktion (Seite, Mip-Stufe, Richtung) -> Farbe
fn cubemap(size: u32, mips: u32, f: impl Fn(u32, Vec3) -> Vec3) -> Image {
    let mut bytes = Vec::new();
    // Reihenfolge: je Seite alle Mip-Stufen
    for face in 0..6 {
        for mip in 0..mips {
            let s = (size >> mip).max(1);
            for y in 0..s {
                for x in 0..s {
                    let u = (x as f32 + 0.5) / s as f32 * 2.0 - 1.0;
                    let v = (y as f32 + 0.5) / s as f32 * 2.0 - 1.0;
                    let c = f(mip, face_dir(face, u, v));
                    for k in [c.x, c.y, c.z, 1.0] {
                        bytes.extend_from_slice(&f16_bits(k).to_le_bytes());
                    }
                }
            }
        }
    }
    let mut image = Image::new_uninit(
        Extent3d { width: size, height: size, depth_or_array_layers: 6 },
        TextureDimension::D2,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = mips;
    image.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), ..default() });
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
    image.data = Some(bytes);
    image
}

/// Streulicht: Himmel über die Halbkugel um die Normale gemittelt (kosinusgewichtet)
fn irradiance(n: Vec3, samples: &[Vec3]) -> Vec3 {
    let mut acc = Vec3::ZERO;
    let mut w = 0.0;
    for s in samples {
        let c = s.dot(n);
        if c > 0.0 {
            acc += sky(*s, 1.0) * c;
            w += c;
        }
    }
    acc / w.max(1e-4)
}

/// Gleichmäßig verteilte Richtungen auf der Kugel (Fibonacci-Spirale)
fn sphere_samples(n: usize) -> Vec<Vec3> {
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    (0..n)
        .map(|i| {
            let y = 1.0 - (i as f32 + 0.5) / n as f32 * 2.0;
            let r = (1.0 - y * y).sqrt();
            let a = golden * i as f32;
            Vec3::new(a.cos() * r, y, a.sin() * r)
        })
        .collect()
}

const SPECULAR_SIZE: u32 = 64;
const SPECULAR_MIPS: u32 = 7;

pub fn environment_light(images: &mut Assets<Image>) -> EnvironmentMapLight {
    let samples = sphere_samples(192);
    let diffuse = cubemap(16, 1, |_, d| irradiance(d, &samples));
    let specular = cubemap(SPECULAR_SIZE, SPECULAR_MIPS, |mip, d| sky(d, mip as f32 / (SPECULAR_MIPS - 1) as f32));
    EnvironmentMapLight {
        diffuse_map: images.add(diffuse),
        specular_map: images.add(specular),
        intensity: 700.0,
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halffloat_roundtrip_is_close() {
        for x in [0.0f32, 0.25, 1.0, 1.5, 7.3, 300.0] {
            let h = f16_bits(x);
            let exp = ((h >> 10) & 0x1f) as i32;
            let man = (h & 0x3ff) as f32;
            let back = if exp == 0 { 0.0 } else { (1.0 + man / 1024.0) * 2f32.powi(exp - 15) };
            assert!((back - x).abs() <= x * 0.002 + 1e-6, "{x} -> {back}");
        }
    }

    #[test]
    fn sky_is_bright_above_and_dark_below() {
        let up = sky(Vec3::Y, 0.0);
        let down = sky(Vec3::NEG_Y, 0.0);
        assert!(up.length() > down.length() * 1.5);
        // blauer Zenit
        assert!(up.z > up.x);
        // Sonne heller als der übrige Himmel, raue Flächen sehen sie nur gedämpft
        assert!(sky(sun_dir(), 0.0).x > sky(sun_dir(), 1.0).x * 5.0);
    }
}
