//! Detaillierte Rohstofffelder.
//!
//! Ferrit: ein Felsausbiss, aus dem facettierte Kristallbündel wachsen, dazu Splitter im
//! Geröll und leuchtende Adern im Boden. Beim Abbau schrumpfen nur die Kristalle.
//! Plasma: ein unregelmäßiger Krater mit Felsrand, Kristallstacheln am Ufer, einem
//! leuchtenden Becken und aufsteigenden Plasmablasen.
//! Jedes Feld wird einmal (mit eigenem Zufallswert) gebaut und zu wenigen Meshes zusammengefasst.

use crate::components::*;
use crate::models::{ModelKit, Pulse};
use crate::terrain;
use crate::vehicles::{lin, Geo};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use std::f32::consts::TAU;

/// Wird beim Abbau mit der verbleibenden Menge skaliert
#[derive(Component)]
pub struct Shrinks;

/// Aufsteigende Plasmablase
#[derive(Component)]
pub struct Rise {
    speed: f32,
    height: f32,
    phase: f32,
    size: f32,
    origin: Vec3,
}

/// Facettierter Kristall: unregelmäßiges Sechskantprisma mit schiefer Spitze
fn crystal(g: &mut Geo, rng: &mut Rng, base: Vec3, dir: Vec3, len: f32, radius: f32, deep: Vec3, light: Vec3) {
    let dir = dir.normalize();
    let rot = Quat::from_rotation_arc(Vec3::Y, dir) * Quat::from_rotation_y(rng.range(0.0, TAU));
    let sides = 6;
    let ring = |r_scale: f32, y: f32, rng: &mut Rng| -> Vec<Vec3> {
        (0..sides)
            .map(|i| {
                let a = TAU * i as f32 / sides as f32;
                let r = radius * r_scale * rng.range(0.85, 1.12);
                base + rot * Vec3::new(a.cos() * r, y, a.sin() * r)
            })
            .collect()
    };
    let a = ring(1.0, -0.15, rng);
    let shoulder = len * rng.range(0.62, 0.78);
    let b = ring(rng.range(0.85, 1.0), shoulder, rng);
    let tip = base + rot * Vec3::new(rng.range(-0.25, 0.25) * radius, len, rng.range(-0.25, 0.25) * radius);
    let axis_mid = base + dir * (shoulder * 0.5);
    for i in 0..sides {
        let j = (i + 1) % sides;
        // Seitenflächen: unten dunkel, oben hell
        let quad = [a[i], a[j], b[j], b[i]];
        let fc = quad.iter().sum::<Vec3>() / 4.0;
        let n = (quad[1] - quad[0]).cross(quad[2] - quad[0]);
        let out = n.dot(fc - axis_mid) > 0.0;
        let (c0, c1) = (deep, deep.lerp(light, 0.65));
        let mut t = |p: [Vec3; 3], c: Vec3| {
            if out {
                g.tri(p[0], p[1], p[2], c);
            } else {
                g.tri(p[0], p[2], p[1], c);
            }
        };
        t([quad[0], quad[1], quad[2]], c0.lerp(c1, 0.4));
        t([quad[0], quad[2], quad[3]], c0.lerp(c1, 0.6));
        // Spitze
        let n2 = (b[j] - b[i]).cross(tip - b[i]);
        let fc2 = (b[i] + b[j] + tip) / 3.0;
        if n2.dot(fc2 - (base + dir * len * 0.8)) > 0.0 {
            g.tri(b[i], b[j], tip, light);
        } else {
            g.tri(b[i], tip, b[j], light);
        }
    }
}

/// Verbeulter Felsbrocken
fn lump(g: &mut Geo, shape: &Mesh, pos: Vec3, scale: Vec3, yaw: f32, col: Vec3) {
    g.mesh(shape.clone(), Transform::from_translation(pos).with_scale(scale).with_rotation(Quat::from_rotation_y(yaw)), col);
}

pub fn spawn_ferrit(c: &mut Commands, k: &ModelKit, meshes: &mut Assets<Mesh>, root: Entity, seed: u32) {
    let mut rng = Rng(0xA5A5_1234 ^ (seed as u64 * 7919 + 1));
    let rock_shapes: Vec<Mesh> = (0..3).map(|i| terrain::rock_mesh(seed * 3 + i + 11)).collect();
    let rock_dark = lin(Color::srgb(0.20, 0.20, 0.21));
    let rock_mid = lin(Color::srgb(0.33, 0.31, 0.29));
    let deep = lin(Color::srgb(0.05, 0.30, 0.26));
    let light = lin(Color::srgb(0.62, 1.0, 0.90));

    // Felsausbiss in der Mitte und Geröll ringsum
    let mut rocks = Geo::default();
    lump(&mut rocks, &rock_shapes[0], Vec3::new(0.0, -0.2, 0.0), Vec3::new(1.9, 0.9, 1.6), rng.range(0.0, TAU), rock_dark);
    for i in 0..7 {
        let a = TAU * i as f32 / 7.0 + rng.range(-0.3, 0.3);
        let r = rng.range(1.4, 2.6);
        let s = rng.range(0.35, 0.8);
        lump(
            &mut rocks,
            &rock_shapes[i % 3],
            Vec3::new(a.cos() * r, -0.1 * s, a.sin() * r),
            Vec3::new(s * rng.range(0.9, 1.4), s * 0.7, s),
            rng.range(0.0, TAU),
            if i % 2 == 0 { rock_mid } else { rock_dark },
        );
    }
    // Splitter im Geröll (wachsen nicht nach, schrumpfen also auch nicht)
    for _ in 0..14 {
        let a = rng.range(0.0, TAU);
        let r = rng.range(1.6, 3.4);
        let p = Vec3::new(a.cos() * r, -0.05, a.sin() * r);
        let dir = Vec3::new(rng.range(-1.0, 1.0), rng.range(0.2, 1.0), rng.range(-1.0, 1.0));
        let (len, rad) = (rng.range(0.15, 0.4), rng.range(0.04, 0.08));
        crystal(&mut rocks, &mut rng, p, dir, len, rad, deep, light);
    }
    rocks.weather(lin(Color::srgb(0.48, 0.42, 0.33)), 0.6, 0.5);
    c.entity(root).with_child((Mesh3d(meshes.add(rocks.build().expect("Fels"))), MeshMaterial3d(k.building_mat.clone())));

    // Kristallbündel: ein großes in der Mitte, kleinere auf dem Fels verteilt
    let mut cg = Geo::default();
    let clusters = [(Vec3::new(0.0, 0.45, 0.0), 1.3, 9), (Vec3::ZERO, 0.8, 5), (Vec3::ZERO, 0.7, 5), (Vec3::ZERO, 0.6, 4)];
    for (ci, (center, scale, count)) in clusters.into_iter().enumerate() {
        let center = if ci == 0 {
            center
        } else {
            let a = TAU * ci as f32 / 3.0 + rng.range(-0.4, 0.4);
            Vec3::new(a.cos() * 1.2, 0.2, a.sin() * 1.2)
        };
        let lean_out = Vec3::new(center.x, 0.0, center.z).normalize_or_zero() * 0.35;
        for i in 0..count {
            let main = i == 0;
            let dir = if main {
                Vec3::Y + lean_out + Vec3::new(rng.range(-0.1, 0.1), 0.0, rng.range(-0.1, 0.1))
            } else {
                let a = rng.range(0.0, TAU);
                Vec3::Y + lean_out + Vec3::new(a.cos(), 0.0, a.sin()) * rng.range(0.35, 0.9)
            };
            let len = scale * if main { rng.range(2.3, 2.8) } else { rng.range(0.9, 1.8) };
            let rad = scale * if main { 0.34 } else { rng.range(0.13, 0.24) };
            let off = Vec3::new(rng.range(-0.25, 0.25), 0.0, rng.range(-0.25, 0.25)) * scale;
            crystal(&mut cg, &mut rng, center + off, dir, len, rad, deep, light);
        }
    }
    let crystals = c
        .spawn((Mesh3d(meshes.add(cg.build().expect("Kristalle"))), MeshMaterial3d(k.ferrit.clone()), Transform::default(), Shrinks))
        .id();
    c.entity(root).add_child(crystals);

    // Leuchtende Adern im Boden, strahlenförmig vom Ausbiss weg
    let mut veins = Geo::default();
    for i in 0..6 {
        let mut a = TAU * i as f32 / 6.0 + rng.range(-0.3, 0.3);
        let mut p = Vec3::new(a.cos() * 1.3, 0.0, a.sin() * 1.3);
        for _ in 0..4 {
            a += rng.range(-0.5, 0.5);
            let step = rng.range(0.5, 0.9);
            let q = p + Vec3::new(a.cos(), 0.0, a.sin()) * step;
            let mid = (p + q) / 2.0;
            let w = rng.range(0.03, 0.07);
            veins.cuboid(mid + Vec3::Y * 0.02, Vec3::new(w, 0.02, step), Quat::from_rotation_y(-a + std::f32::consts::FRAC_PI_2), Vec3::ONE);
            p = q;
        }
    }
    c.entity(root).with_child((
        Mesh3d(meshes.add(veins.build().expect("Adern"))),
        MeshMaterial3d(k.ferrit_glow.clone()),
        NotShadowCaster,
    ));

    c.entity(root).with_child((
        PointLight {
            color: Color::srgb(0.4, 1.0, 0.8),
            intensity: 300_000.0,
            range: 10.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 2.4, 0.0),
    ));
}

pub fn spawn_plasma(c: &mut Commands, k: &ModelKit, meshes: &mut Assets<Mesh>, root: Entity, seed: u32) {
    let mut rng = Rng(0x5EED_0F00 ^ (seed as u64 * 104_729 + 3));
    let scorched = lin(Color::srgb(0.17, 0.15, 0.15));
    let ash = lin(Color::srgb(0.30, 0.27, 0.26));
    let rock = lin(Color::srgb(0.26, 0.24, 0.24));

    // Kraterwall: um die Achse gedrehtes Profil (Radius, Höhe), am Rand verbeult
    let mut g = Geo::default();
    let prof = [(1.3, 0.03), (1.5, 0.2), (1.8, 0.42), (2.25, 0.5), (2.8, 0.28), (3.4, 0.0), (3.7, -0.12)];
    let segs = 36;
    let wobble: Vec<f32> = (0..segs).map(|i| 1.0 + (i as f32 * 0.9 + seed as f32).sin() * 0.06 + rng.range(-0.05, 0.05)).collect();
    let point = |i: usize, j: usize| {
        let a = TAU * i as f32 / segs as f32;
        let (r, y) = prof[j];
        let w = wobble[i % segs];
        let lip = if (2..=4).contains(&j) { w } else { 1.0 };
        Vec3::new(a.cos() * r * w, y * lip, a.sin() * r * w)
    };
    for i in 0..segs {
        for j in 0..prof.len() - 1 {
            let (p00, p10, p01, p11) = (point(i, j), point(i + 1, j), point(i, j + 1), point(i + 1, j + 1));
            // Innenseite verbrannt, außen Asche
            let col = if j < 2 { scorched } else { scorched.lerp(ash, (j as f32 - 1.0) / 4.0) };
            g.tri(p00, p11, p01, col);
            g.tri(p00, p10, p11, col);
        }
    }
    // Felsen auf dem Wall
    let shapes: Vec<Mesh> = (0..3).map(|i| terrain::rock_mesh(seed * 5 + i + 31)).collect();
    for i in 0..9 {
        let a = TAU * i as f32 / 9.0 + rng.range(-0.25, 0.25);
        let r = rng.range(2.0, 3.0);
        let s = rng.range(0.25, 0.6);
        lump(&mut g, &shapes[i % 3], Vec3::new(a.cos() * r, 0.3, a.sin() * r), Vec3::new(s * 1.3, s * 0.8, s), rng.range(0.0, TAU), rock);
    }
    g.weather(lin(Color::srgb(0.45, 0.40, 0.33)), 0.5, 0.25);
    c.entity(root).with_child((Mesh3d(meshes.add(g.build().expect("Krater"))), MeshMaterial3d(k.building_mat.clone())));

    // Kristallstacheln am Ufer (violett leuchtend)
    let mut spikes = Geo::default();
    let deep = lin(Color::srgb(0.20, 0.05, 0.30));
    let light = lin(Color::srgb(0.95, 0.55, 1.0));
    for _ in 0..10 {
        let a = rng.range(0.0, TAU);
        let r = rng.range(1.2, 1.6);
        let p = Vec3::new(a.cos() * r, -0.05, a.sin() * r);
        let dir = Vec3::new(a.cos(), 0.0, a.sin()) * rng.range(-0.6, 0.3) + Vec3::Y;
        let (len, rad) = (rng.range(0.35, 0.9), rng.range(0.05, 0.11));
        crystal(&mut spikes, &mut rng, p, dir, len, rad, deep, light);
    }
    c.entity(root).with_child((Mesh3d(meshes.add(spikes.build().expect("Stacheln"))), MeshMaterial3d(k.plasma_crystal.clone())));

    // Leuchtendes Becken: zwei Schichten, die innere heller
    c.entity(root).with_child((
        Mesh3d(k.disc.clone()),
        MeshMaterial3d(k.plasma.clone()),
        Transform::from_xyz(0.0, 0.06, 0.0).with_scale(Vec3::new(1.55, 1.0, 1.55)),
        NotShadowCaster,
        NotShadowReceiver,
    ));
    c.entity(root).with_child((
        Mesh3d(k.disc.clone()),
        MeshMaterial3d(k.plasma_core.clone()),
        Transform::from_xyz(0.0, 0.08, 0.0).with_scale(Vec3::new(0.8, 1.0, 0.8)),
        NotShadowCaster,
        NotShadowReceiver,
    ));

    // Aufsteigende Blasen
    for i in 0..6 {
        let a = rng.range(0.0, TAU);
        let r = rng.range(0.0, 1.0);
        c.entity(root).with_child((
            Mesh3d(k.bubble.clone()),
            MeshMaterial3d(k.plasma_core.clone()),
            Transform::default(),
            NotShadowCaster,
            Rise {
                speed: rng.range(0.4, 0.8),
                height: rng.range(1.5, 2.8),
                phase: i as f32 * 0.37 + rng.next_f32(),
                size: rng.range(0.08, 0.16),
                origin: Vec3::new(a.cos() * r, 0.0, a.sin() * r),
            },
        ));
    }

    c.entity(root).with_child((
        PointLight {
            color: Color::srgb(0.8, 0.35, 1.0),
            intensity: 700_000.0,
            range: 12.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 1.5, 0.0),
        Pulse { base: 700_000.0, phase: seed as f32 * 1.7 },
    ));
}

pub fn rise(time: Res<Time>, mut q: Query<(&Rise, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (r, mut tf) in &mut q {
        let k = ((t * r.speed / r.height + r.phase).fract()).clamp(0.0, 1.0);
        let sway = Vec3::new((t * 1.7 + r.phase * 5.0).sin(), 0.0, (t * 1.3 + r.phase * 3.0).cos()) * 0.12 * k;
        tf.translation = r.origin + sway + Vec3::Y * k * r.height;
        // wachsen, dann verglühen
        tf.scale = Vec3::splat(r.size * (k * 4.0).min(1.0) * (1.0 - k).powf(0.7) * 2.0);
    }
}

/// Kristalle schrumpfen, wenn das Feld abgebaut wird
pub fn shrink_nodes(
    nodes: Query<(&ResourceNode, &Children), Changed<ResourceNode>>,
    mut parts: Query<&mut Transform, With<Shrinks>>,
) {
    for (n, children) in &nodes {
        let r = n.amount as f32 / n.max.max(1) as f32;
        for &child in children {
            if let Ok(mut t) = parts.get_mut(child) {
                t.scale = Vec3::new(0.3 + 0.7 * r, 0.2 + 0.8 * r, 0.3 + 0.7 * r);
            }
        }
    }
}
