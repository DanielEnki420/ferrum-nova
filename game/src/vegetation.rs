//! Vegetation: Grasbüschel, Büsche und Kiefern, alles prozedural.
//!
//! Alles ist statisch und wird pro Kartenkachel zu einem Mesh zusammengefasst, damit
//! WebGL2 nur wenige Zeichenaufrufe braucht. Der Wind bewegt die Vertices im Shader
//! (`uv.y` = Biegsamkeit). Gras wirft keine Schatten, Büsche und Bäume schon.

use crate::components::*;
use crate::terrain::{self, FIELDS, TERRAIN_HALF};
use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use std::f32::consts::TAU;

pub type WindMaterial = ExtendedMaterial<StandardMaterial, WindExt>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct WindExt {
    /// x: Stärke, y: Tempo, z: Flattern, w: frei
    #[uniform(100)]
    pub params: Vec4,
    /// Nebel des Krieges (fog.rs)
    #[texture(101)]
    #[sampler(102)]
    pub fog: Handle<Image>,
}

impl WindExt {
    pub fn new(params: Vec4) -> Self {
        Self { params, fog: crate::fog::FOG_TEXTURE }
    }
}

impl MaterialExtension for WindExt {
    fn vertex_shader() -> ShaderRef {
        "shaders/wind.wesl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/wind_fragment.wesl".into()
    }
}

const CHUNKS: usize = 8;

/// Sammelt Geometrie pro Kachel
struct Batch {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl Batch {
    fn new() -> Self {
        Self { pos: vec![], nrm: vec![], uv: vec![], col: vec![], idx: vec![] }
    }

    fn vert(&mut self, p: Vec3, n: Vec3, bend: f32, c: Vec3) -> u32 {
        self.pos.push(p.to_array());
        self.nrm.push(n.to_array());
        self.uv.push([0.0, bend]);
        self.col.push([c.x, c.y, c.z, 1.0]);
        (self.pos.len() - 1) as u32
    }

    /// Fügt ein Bevy-Primitiv ein (transformiert, eingefärbt, mit Biegsamkeit nach Höhe)
    fn add_mesh(&mut self, mesh: Mesh, tf: Transform, color: impl Fn(Vec3) -> Vec3, bend: impl Fn(Vec3) -> f32) {
        let m = mesh.transformed_by(tf);
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else { return };
        let Some(VertexAttributeValues::Float32x3(n)) = m.attribute(Mesh::ATTRIBUTE_NORMAL) else { return };
        let base = self.pos.len() as u32;
        for (p, n) in p.iter().zip(n) {
            let v = Vec3::from_array(*p);
            self.vert(v, Vec3::from_array(*n), bend(v), color(v));
        }
        match m.indices() {
            Some(Indices::U32(i)) => self.idx.extend(i.iter().map(|i| i + base)),
            Some(Indices::U16(i)) => self.idx.extend(i.iter().map(|&i| i as u32 + base)),
            None => {}
        }
    }

    fn into_mesh(self) -> Option<Mesh> {
        if self.idx.is_empty() {
            return None;
        }
        Some(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
                .with_inserted_indices(Indices::U32(self.idx)),
        )
    }
}

fn chunk_of(x: f32, z: f32) -> usize {
    let f = |v: f32| (((v + TERRAIN_HALF) / (2.0 * TERRAIN_HALF) * CHUNKS as f32) as usize).min(CHUNKS - 1);
    f(z) * CHUNKS + f(x)
}

fn srgb(r: f32, g: f32, b: f32) -> Vec3 {
    let c = LinearRgba::from(Color::srgb(r, g, b));
    Vec3::new(c.red, c.green, c.blue)
}

/// Kein Bewuchs auf Basen, Rohstofffeldern und Fahrwegen dazwischen
fn blocked(x: f32, z: f32) -> bool {
    let p = Vec3::new(x, 0.0, z);
    if p.distance(PLAYER_HQ_POS) < 13.0 || p.distance(ENEMY_HQ_POS) < 13.0 {
        return true;
    }
    FIELDS.iter().any(|(f, _, _)| p.distance(*f) < 5.5) || terrain::track_weight(x, z) > 0.3
}

/// Ein Grasbüschel aus mehreren gebogenen Halmen
fn tuft(b: &mut Batch, rng: &mut Rng, root: Vec3, size: f32, dry: f32) {
    let blades = 5 + (rng.next_f32() * 5.0) as usize;
    let green_base = srgb(0.17, 0.19, 0.08);
    let green_tip = srgb(0.55, 0.53, 0.27);
    let straw_base = srgb(0.30, 0.25, 0.15);
    let straw_tip = srgb(0.74, 0.65, 0.44);
    for _ in 0..blades {
        let a = rng.range(0.0, TAU);
        let off = Vec3::new(a.cos(), 0.0, a.sin()) * rng.range(0.0, 0.22) * size;
        let p0 = root + off;
        let h = rng.range(0.35, 0.75) * size;
        let w = rng.range(0.035, 0.06) * size;
        let yaw = rng.range(0.0, TAU);
        let side = Vec3::new(yaw.cos(), 0.0, yaw.sin());
        // nach außen geneigt
        let lean = (off.normalize_or_zero() + Vec3::new(rng.range(-0.4, 0.4), 0.0, rng.range(-0.4, 0.4))) * h * 0.45;
        let face = side.cross(Vec3::Y).normalize();
        let n = (Vec3::Y * 1.4 + face * 0.4).normalize();
        let dry_k = (dry + rng.range(-0.2, 0.2)).clamp(0.0, 1.0);
        let base_c = green_base.lerp(straw_base, dry_k);
        let tip_c = green_tip.lerp(straw_tip, dry_k) * rng.range(0.8, 1.1);
        let at = |t: f32| p0 + Vec3::Y * h * t + lean * t * t;
        let v0 = b.vert(at(0.0) - side * w, n, 0.0, base_c);
        let v1 = b.vert(at(0.0) + side * w, n, 0.0, base_c);
        let v2 = b.vert(at(0.5) - side * w * 0.7, n, 0.5, base_c.lerp(tip_c, 0.55));
        let v3 = b.vert(at(0.5) + side * w * 0.7, n, 0.5, base_c.lerp(tip_c, 0.55));
        let v4 = b.vert(at(1.0), n, 1.0, tip_c);
        b.idx.extend_from_slice(&[v0, v1, v2, v1, v3, v2, v2, v3, v4]);
    }
}

/// Ein Busch: mehrere verbeulte Kugeln, unten dunkler
fn bush(b: &mut Batch, rng: &mut Rng, root: Vec3, size: f32, sphere: &Mesh) {
    let dark = srgb(0.10, 0.13, 0.06);
    let light = srgb(0.32, 0.33, 0.16).lerp(srgb(0.40, 0.34, 0.20), rng.next_f32() * 0.6);
    for _ in 0..(3 + (rng.next_f32() * 3.0) as usize) {
        let off = Vec3::new(rng.range(-0.5, 0.5), rng.range(0.2, 0.6), rng.range(-0.5, 0.5)) * size;
        let s = size * rng.range(0.45, 0.75);
        let tf = Transform::from_translation(root + off)
            .with_scale(Vec3::new(s, s * 0.8, s))
            .with_rotation(Quat::from_rotation_y(rng.range(0.0, TAU)));
        let top = root.y + size * 1.2;
        b.add_mesh(
            sphere.clone(),
            tf,
            |v| dark.lerp(light, ((v.y - root.y) / (top - root.y)).clamp(0.0, 1.0)),
            |v| ((v.y - root.y) / (top - root.y)).clamp(0.0, 1.0) * 0.35,
        );
    }
}

/// Eine Kiefer: Stamm und gestaffelte, leicht verdrehte Kegel
fn pine(b: &mut Batch, rng: &mut Rng, root: Vec3, height: f32) {
    let bark = srgb(0.20, 0.15, 0.11);
    let needle_dark = srgb(0.05, 0.09, 0.05);
    let needle_light = srgb(0.17, 0.24, 0.12).lerp(srgb(0.22, 0.23, 0.13), rng.next_f32());
    let trunk_h = height * 0.35;
    b.add_mesh(
        Cylinder::new(height * 0.035, trunk_h + 0.3).mesh().resolution(6).build(),
        Transform::from_translation(root + Vec3::Y * (trunk_h * 0.5 - 0.15)),
        |_| bark,
        |_| 0.0,
    );
    let tiers = 4 + (rng.next_f32() * 2.0) as usize;
    for i in 0..tiers {
        let k = i as f32 / tiers as f32;
        let r = height * (0.30 - k * 0.20) * rng.range(0.9, 1.1);
        let ch = height * 0.34 * (1.0 - k * 0.35);
        let y = trunk_h * 0.6 + k * (height - trunk_h * 0.6 - ch * 0.6);
        let tilt = Quat::from_rotation_x(rng.range(-0.06, 0.06)) * Quat::from_rotation_z(rng.range(-0.06, 0.06));
        let tf = Transform::from_translation(root + Vec3::Y * (y + ch * 0.5)).with_rotation(tilt * Quat::from_rotation_y(rng.range(0.0, TAU)));
        let (y0, y1) = (root.y + y, root.y + y + ch);
        b.add_mesh(
            Cone::new(r, ch).mesh().resolution(9).build(),
            tf,
            move |v| needle_dark.lerp(needle_light, ((v.y - y0) / (y1 - y0)).clamp(0.0, 1.0) * 0.5 + 0.5 * k),
            move |v| ((v.y - root.y) / height).clamp(0.0, 1.0) * 0.5,
        );
    }
}

pub fn setup_vegetation(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<WindMaterial>>,
) {
    let mut rng = Rng(0xA076_1D64_78BD_642F);
    let mut grass: Vec<Batch> = (0..CHUNKS * CHUNKS).map(|_| Batch::new()).collect();
    let mut plants: Vec<Batch> = (0..CHUNKS * CHUNKS).map(|_| Batch::new()).collect();
    let mut sphere = Sphere::new(1.0).mesh().ico(1).expect("Ikosphäre");
    // verbeulen, damit die Büsche nicht wie Kugeln aussehen
    if let Some(VertexAttributeValues::Float32x3(p)) = sphere.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
        for (i, v) in p.iter_mut().enumerate() {
            let k = 1.0 + ((i as f32 * 12.9898).sin() * 43_758.547).fract() * 0.35 - 0.12;
            *v = (Vec3::from_array(*v) * k).to_array();
        }
    }
    sphere.compute_smooth_normals();

    // Gras auf einem gestörten Raster
    let spacing = 0.75;
    let lim = MAP_HALF + 22.0;
    let steps = (2.0 * lim / spacing) as i32;
    let mut tufts = 0;
    for iz in 0..steps {
        for ix in 0..steps {
            let x = -lim + (ix as f32 + rng.next_f32()) * spacing;
            let z = -lim + (iz as f32 + rng.next_f32()) * spacing;
            if blocked(x, z) {
                continue;
            }
            let w = terrain::weights_at(x, z);
            let (dirt, gravel, rock, gr) = (w[0], w[1], w[2], w[3]);
            // dichtes Gras in den grünen Zonen, vereinzelte trockene Büschel überall sonst
            let expected = gr * 3.2 + dirt * 0.05 + gravel * 0.03 - rock * 0.5;
            let count = (expected + rng.next_f32()).floor().max(0.0) as usize;
            for k in 0..count {
                let (px, pz) = if k == 0 {
                    (x, z)
                } else {
                    (x + rng.range(-0.4, 0.4) * spacing, z + rng.range(-0.4, 0.4) * spacing)
                };
                let root = Vec3::new(px, terrain::height(px, pz) - 0.03, pz);
                let size = rng.range(0.8, 1.4) * if gr > 0.3 { 1.1 } else { 0.8 };
                tuft(&mut grass[chunk_of(px, pz)], &mut rng, root, size, 1.0 - gr * 1.3);
                tufts += 1;
            }
        }
    }

    // Büsche und Bäume: am Rand der grünen Zonen und an den Berghängen
    let mut trees = 0;
    let mut bushes = 0;
    let mut tries = 0;
    while (trees < 140 || bushes < 160) && tries < 40_000 {
        tries += 1;
        let x = rng.range(-TERRAIN_HALF + 10.0, TERRAIN_HALF - 10.0);
        let z = rng.range(-TERRAIN_HALF + 10.0, TERRAIN_HALF - 10.0);
        if blocked(x, z) {
            continue;
        }
        let h = terrain::height(x, z);
        let n = terrain::normal(x, z);
        let w = terrain::weights_at(x, z);
        let edge = x.abs().max(z.abs());
        let slope = 1.0 - n.y;
        let root = Vec3::new(x, h - 0.1, z);
        // Kiefern: Hänge des Randgebirges (nicht zu steil, nicht zu hoch) und Haine im Gras
        let foothill = edge > MAP_HALF - 6.0 && h < 22.0 && slope < 0.35;
        let grove = w[3] > 0.45 && edge < MAP_HALF - 8.0;
        if trees < 140 && (foothill && rng.next_f32() < 0.5 || grove && rng.next_f32() < 0.08) {
            // kleine Gruppen
            for _ in 0..1 + (rng.next_f32() * 3.0) as usize {
                let p = root + Vec3::new(rng.range(-3.0, 3.0), 0.0, rng.range(-3.0, 3.0));
                let p = Vec3::new(p.x, terrain::height(p.x, p.z) - 0.1, p.z);
                if blocked(p.x, p.z) {
                    continue;
                }
                let th = rng.range(5.0, 9.5);
                pine(&mut plants[chunk_of(p.x, p.z)], &mut rng, p, th);
                trees += 1;
            }
            continue;
        }
        if bushes < 160 && (w[3] > 0.25 || slope > 0.08 && slope < 0.3) && edge < MAP_HALF + 15.0 && rng.next_f32() < 0.3 {
            let bs = rng.range(0.6, 1.3);
            bush(&mut plants[chunk_of(x, z)], &mut rng, root, bs, &sphere);
            bushes += 1;
        }
    }
    info!("Vegetation: {tufts} Grasbüschel, {bushes} Büsche, {trees} Bäume");

    let grass_mat = mats.add(ExtendedMaterial {
        base: StandardMaterial {
            perceptual_roughness: 0.85,
            reflectance: 0.2,
            double_sided: true,
            cull_mode: None,
            ..default()
        },
        extension: WindExt::new(Vec4::new(0.22, 1.0, 0.25, 0.0)),
    });
    let plant_mat = mats.add(ExtendedMaterial {
        base: StandardMaterial { perceptual_roughness: 0.9, reflectance: 0.2, ..default() },
        extension: WindExt::new(Vec4::new(0.12, 0.8, 0.1, 0.0)),
    });
    for m in grass.into_iter().filter_map(Batch::into_mesh) {
        commands.spawn((Mesh3d(meshes.add(m)), MeshMaterial3d(grass_mat.clone()), NotShadowCaster));
    }
    for m in plants.into_iter().filter_map(Batch::into_mesh) {
        commands.spawn((Mesh3d(meshes.add(m)), MeshMaterial3d(plant_mat.clone())));
    }
}
