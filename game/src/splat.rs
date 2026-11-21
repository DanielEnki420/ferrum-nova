//! Gelände-Material: vier echte Bodentexturen (CC0, ambientCG) als Texture-Arrays.
//!
//! Die Mischgewichte (Erde, Geröll, Fels, Gras) stehen pro Vertex in der Vertexfarbe.
//! Der Shader mischt die Schichten höhenbasiert, legt Fels dreiplanar auf steile Flanken
//! und bricht die Kachelung mit einer zweiten, gedrehten Abtastung auf.

use crate::terrain;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;

pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainExt>;

const SHADER: &str = "shaders/terrain.wesl";
/// Reihenfolge der Schichten im Array (und der Gewichte in der Vertexfarbe)
const LAYERS: [&str; 4] = ["dirt", "gravel", "rock", "grass"];

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct TerrainExt {
    /// x: Kachelung Boden (1/m), y: Makro-Variation, z: Normalstärke, w: Kachelung Fels
    #[uniform(100)]
    pub params: Vec4,
    #[texture(101, dimension = "2d_array")]
    #[sampler(102)]
    pub albedo: Handle<Image>,
    #[texture(103, dimension = "2d_array")]
    pub normal: Handle<Image>,
    #[texture(104, dimension = "2d_array")]
    pub arm: Handle<Image>,
    /// Nebel des Krieges (fog.rs)
    #[texture(105)]
    #[sampler(106)]
    pub fog: Handle<Image>,
}

impl MaterialExtension for TerrainExt {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

/// Die geladenen Einzelbilder, bis daraus die Arrays gebaut sind
#[derive(Resource)]
pub struct PendingTerrain {
    color: Vec<Handle<Image>>,
    normal: Vec<Handle<Image>>,
    rough: Vec<Handle<Image>>,
    ao: Vec<Handle<Image>>,
}

pub fn load_terrain_textures(mut commands: Commands, server: Res<AssetServer>) {
    let linear = |name: &str| {
        server
            .load_builder()
            .with_settings(|s: &mut ImageLoaderSettings| s.is_srgb = false)
            .load(format!("terrain/{name}.jpg"))
    };
    commands.insert_resource(PendingTerrain {
        color: LAYERS.iter().map(|l| server.load(format!("terrain/{l}_color.jpg"))).collect(),
        normal: LAYERS.iter().map(|l| linear(&format!("{l}_normal"))).collect(),
        rough: LAYERS.iter().map(|l| linear(&format!("{l}_rough"))).collect(),
        ao: LAYERS.iter().map(|l| linear(&format!("{l}_ao"))).collect(),
    });
}

/// Sobald alle Bilder da sind: Arrays bauen und das Gelände erzeugen.
/// Fehlt etwas, kommt das alte, rein prozedurale Gelände.
pub fn finish_terrain(
    mut commands: Commands,
    pending: Option<Res<PendingTerrain>>,
    server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut terrain_mats: ResMut<Assets<TerrainMaterial>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
) {
    let Some(p) = pending else { return };
    let all = || p.color.iter().chain(&p.normal).chain(&p.rough).chain(&p.ao);
    if all().any(|h| server.load_state(h).is_failed()) {
        warn!("Geländetexturen fehlen, nutze prozedurales Gelände");
        commands.remove_resource::<PendingTerrain>();
        spawn_fallback(&mut commands, &mut meshes, &mut images, &mut std_mats);
        return;
    }
    if !all().all(|h| images.contains(h)) {
        return;
    }

    let pixels = |h: &Handle<Image>| -> (u32, Vec<u8>) {
        let img = images.get(h).expect("geladen");
        let size = img.texture_descriptor.size.width;
        (size, to_rgba(img))
    };
    let color: Vec<_> = p.color.iter().map(pixels).collect();
    let normal: Vec<_> = p.normal.iter().map(pixels).collect();
    // ARM-Packung: R = Umgebungsverdeckung, G = Rauheit
    let arm: Vec<_> = p
        .rough
        .iter()
        .zip(&p.ao)
        .map(|(r, a)| {
            let (size, r) = pixels(r);
            let (_, a) = pixels(a);
            let mut out = vec![255u8; r.len()];
            for i in 0..r.len() / 4 {
                out[i * 4] = a[i * 4];
                out[i * 4 + 1] = r[i * 4];
                out[i * 4 + 2] = 0;
            }
            (size, out)
        })
        .collect();

    let ext = TerrainExt {
        params: Vec4::new(1.0 / 4.0, 1.0 / 70.0, 1.8, 1.0 / 7.0),
        albedo: images.add(array_image(&color, TextureFormat::Rgba8UnormSrgb)),
        normal: images.add(array_image(&normal, TextureFormat::Rgba8Unorm)),
        arm: images.add(array_image(&arm, TextureFormat::Rgba8Unorm)),
        fog: crate::fog::FOG_TEXTURE,
    };
    let mat = terrain_mats.add(ExtendedMaterial {
        base: StandardMaterial { perceptual_roughness: 0.9, reflectance: 0.3, ..default() },
        extension: ext,
    });
    commands.spawn((Mesh3d(meshes.add(terrain::build_mesh())), MeshMaterial3d(mat), Transform::default(), NotShadowCaster));
    commands.remove_resource::<PendingTerrain>();
}

fn spawn_fallback(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    mats: &mut Assets<StandardMaterial>,
) {
    let (albedo, normal_map) = terrain::build_textures();
    let mut mesh = terrain::build_mesh();
    // die Vertexfarben sind hier Mischgewichte, keine Farben
    mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR);
    if let Err(e) = mesh.generate_tangents() {
        warn!("Tangenten für das Gelände fehlgeschlagen: {e:?}");
    }
    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(mats.add(StandardMaterial {
            base_color: Color::srgb(0.50, 0.46, 0.39),
            base_color_texture: Some(images.add(albedo)),
            normal_map_texture: Some(images.add(normal_map)),
            perceptual_roughness: 0.96,
            reflectance: 0.25,
            ..default()
        })),
        Transform::default(),
        NotShadowCaster,
    ));
}

/// Pixel eines geladenen Bildes als RGBA8 (Graustufen werden aufgefächert)
fn to_rgba(img: &Image) -> Vec<u8> {
    let data = img.data.as_deref().unwrap_or(&[]);
    let px = (img.texture_descriptor.size.width * img.texture_descriptor.size.height) as usize;
    match data.len() / px.max(1) {
        4 => data.to_vec(),
        1 => data.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        2 => data.chunks(2).flat_map(|c| [c[0], c[0], c[0], 255]).collect(),
        n => {
            warn!("Unerwartetes Pixelformat ({n} Byte pro Pixel)");
            vec![128; px * 4]
        }
    }
}

/// Baut ein 2D-Texture-Array mit vollständiger Mipmap-Kette (Schicht für Schicht).
fn array_image(layers: &[(u32, Vec<u8>)], format: TextureFormat) -> Image {
    let size = layers.iter().map(|l| l.0).min().unwrap_or(1);
    let mut data = Vec::new();
    let mut mips = 0;
    for (s, px) in layers {
        // Größere Schichten notfalls auf die kleinste herunterrechnen
        let mut level = px.clone();
        let mut cur = *s;
        while cur > size {
            level = downsample(&level, cur);
            cur /= 2;
        }
        mips = 0;
        loop {
            data.extend_from_slice(&level);
            mips += 1;
            if cur == 1 {
                break;
            }
            level = downsample(&level, cur);
            cur /= 2;
        }
    }
    let mut image = Image::new_uninit(
        Extent3d { width: size, height: size, depth_or_array_layers: layers.len() as u32 },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = mips;
    image.data = Some(data);
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    image
}

fn downsample(px: &[u8], size: u32) -> Vec<u8> {
    let s = size as usize;
    let n = s / 2;
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            for k in 0..4 {
                let at = |dx: usize, dy: usize| px[((y * 2 + dy) * s + x * 2 + dx) * 4 + k] as u32;
                out[(y * n + x) * 4 + k] = ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8;
            }
        }
    }
    out
}
