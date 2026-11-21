//! Detailliertes Hauptquartier.
//!
//! Wie bei den Fahrzeugen entsteht alles aus echter Geometrie und wird zu wenigen Meshes
//! zusammengefasst: Baukörper, Leuchtteile, Glas und die drehende Radarschüssel.
//! Das Tor liegt auf +Z, der Ursprung auf dem Boden in der Mitte.

use crate::components::*;
use crate::vehicles::{lin, palette, Geo};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, TAU};

#[derive(Clone)]
pub struct HqMeshes {
    /// Lage der Radarschüssel relativ zum HQ
    pub dish_pos: Vec3,
    pub body: Handle<Mesh>,
    pub glow: Handle<Mesh>,
    pub glass: Handle<Mesh>,
    pub dish: Handle<Mesh>,
}

/// Lässt ein Bauteil um die Hochachse kreisen (Radar)
#[derive(Component)]
pub struct Spin(pub f32);

pub fn spin(time: Res<Time>, mut q: Query<(&Spin, &mut Transform)>) {
    for (s, mut t) in &mut q {
        t.rotate_y(s.0 * time.delta_secs());
    }
}

/// Achteckiger Grundriss mit abgeschrägten Ecken
fn chamfered(w: f32, d: f32, c: f32) -> Vec<Vec2> {
    let (x, z) = (w / 2.0, d / 2.0);
    vec![
        Vec2::new(-x + c, -z),
        Vec2::new(x - c, -z),
        Vec2::new(x, -z + c),
        Vec2::new(x, z - c),
        Vec2::new(x - c, z),
        Vec2::new(-x + c, z),
        Vec2::new(-x, z - c),
        Vec2::new(-x, -z + c),
    ]
}

/// Warnstreifen gelb/schwarz entlang x
fn hazard(g: &mut Geo, from: Vec3, len: f32, n: usize) {
    let yellow = lin(Color::srgb(0.78, 0.62, 0.12));
    let black = lin(Color::srgb(0.06, 0.06, 0.06));
    let w = len / n as f32;
    for i in 0..n {
        let col = if i % 2 == 0 { yellow } else { black };
        g.cuboid(from + Vec3::X * (w * (i as f32 + 0.5)), Vec3::new(w, 0.012, 0.22), Quat::from_rotation_y(0.0), col);
    }
}

/// Geländer aus Pfosten und Handlauf um einen Kreis
fn ring_rail(g: &mut Geo, center: Vec3, r: f32, posts: usize, col: Vec3) {
    for i in 0..posts {
        let a = TAU * i as f32 / posts as f32;
        g.cyl(center + Vec3::new(a.cos() * r, 0.25, a.sin() * r), 0.02, 0.5, Vec3::Y, 4, col);
    }
    g.mesh(Torus::new(r - 0.02, r + 0.02).mesh().minor_resolution(4).major_resolution(posts * 2).build(), Transform::from_translation(center + Vec3::Y * 0.5), col);
}

pub fn build_hq(team: Team, meshes: &mut Assets<Mesh>) -> HqMeshes {
    let p = palette(team);
    let concrete = lin(Color::srgb(0.50, 0.49, 0.46));
    let concrete_dark = lin(Color::srgb(0.34, 0.33, 0.31));
    let mut g = Geo::default();
    let mut glow = Geo::default();
    let mut glass = Geo::default();
    let base = 0.45;

    // --- Fundament: abgeschrägte Betonplatte mit Fugen und Auffahrt
    g.extrude_y(&chamfered(9.8, 9.8, 1.2), 0.0, base, 0.97, 0.0, concrete);
    for k in -2..=2 {
        let o = k as f32 * 1.9;
        g.cuboid(Vec3::new(o, base + 0.005, 0.0), Vec3::new(0.04, 0.01, 9.2), Quat::IDENTITY, concrete_dark);
        g.cuboid(Vec3::new(0.0, base + 0.005, o), Vec3::new(9.2, 0.01, 0.04), Quat::IDENTITY, concrete_dark);
    }
    let ramp = [Vec2::new(4.75, 0.0), Vec2::new(4.75, base), Vec2::new(6.2, 0.02)];
    g.extrude_x(&ramp, -1.6, 1.6, concrete);
    hazard(&mut g, Vec3::new(-1.6, base + 0.01, 4.55), 3.2, 10);

    // --- Hauptgebäude: leicht geneigte Wände, Pfeiler, Paneelfugen
    let (bw, bd, bz) = (6.2, 4.8, -0.7);
    let plan: Vec<Vec2> = chamfered(bw, bd, 0.5).into_iter().map(|v| v + Vec2::new(0.0, bz)).collect();
    g.extrude_y(&plan, base, 2.95, 0.96, bz * 0.04, p.paint);
    // Sockel in dunkler Farbe
    g.extrude_y(&chamfered(bw + 0.12, bd + 0.12, 0.55).into_iter().map(|v| v + Vec2::new(0.0, bz)).collect::<Vec<_>>(), base, base + 0.4, 1.0, 0.0, p.paint_dark);
    // Pfeiler an den Längsseiten
    for i in 0..5 {
        let x = -2.4 + i as f32 * 1.2;
        for (z, s) in [(bz - bd / 2.0 - 0.04, -1.0f32), (bz + bd / 2.0 + 0.04, 1.0)] {
            if s > 0.0 && x.abs() < 1.6 {
                continue; // Platz für das Tor
            }
            g.cuboid(Vec3::new(x, 1.7, z), Vec3::new(0.22, 2.5, 0.16), Quat::IDENTITY, p.paint_dark);
        }
    }
    // Paneelfugen
    for y in [1.35, 2.35] {
        g.cuboid(Vec3::new(0.0, y, bz), Vec3::new(bw * 0.985, 0.03, bd * 0.985), Quat::IDENTITY, p.paint_dark);
    }
    // Fensterband: leuchtend, darüber ein Sonnenschutz
    glow.cuboid(Vec3::new(0.0, 2.05, bz), Vec3::new(bw * 0.97 + 0.02, 0.22, bd * 0.97 + 0.02), Quat::IDENTITY, Vec3::ONE);
    g.cuboid(Vec3::new(0.0, 2.24, bz), Vec3::new(bw + 0.2, 0.05, bd + 0.2), Quat::IDENTITY, p.steel_dark);
    // Dachkante (Attika)
    g.extrude_y(&chamfered(bw * 0.96 + 0.1, bd * 0.96 + 0.1, 0.5).into_iter().map(|v| v + Vec2::new(0.0, bz * 0.96)).collect::<Vec<_>>(), 2.95, 3.12, 1.0, 0.0, p.paint_dark);

    // --- Tor: Rolltor aus Lamellen mit Rahmen und Lampen
    let gz = bz + bd / 2.0 + 0.03;
    g.cuboid(Vec3::new(0.0, base + 0.95, gz - 0.02), Vec3::new(2.8, 1.9, 0.05), Quat::IDENTITY, p.steel_dark);
    for k in 0..9 {
        g.cuboid(Vec3::new(0.0, base + 0.12 + k as f32 * 0.2, gz + 0.02), Vec3::new(2.6, 0.14, 0.03), Quat::IDENTITY, p.metal);
    }
    for x in [-1.42, 1.42] {
        g.cuboid(Vec3::new(x, base + 1.0, gz + 0.04), Vec3::new(0.12, 2.0, 0.1), Quat::IDENTITY, p.paint_dark);
        g.cuboid(Vec3::new(x, base + 0.5, gz + 0.1), Vec3::new(0.14, 1.0, 0.02), Quat::IDENTITY, lin(Color::srgb(0.78, 0.62, 0.12)));
    }
    g.cuboid(Vec3::new(0.0, base + 2.0, gz + 0.06), Vec3::new(3.0, 0.14, 0.14), Quat::IDENTITY, p.paint_dark);
    for x in [-0.9, 0.0, 0.9] {
        g.cuboid(Vec3::new(x, base + 2.12, gz + 0.12), Vec3::new(0.24, 0.08, 0.14), Quat::IDENTITY, p.steel_dark);
        glow.cuboid(Vec3::new(x, base + 2.08, gz + 0.19), Vec3::new(0.18, 0.03, 0.01), Quat::IDENTITY, Vec3::ONE);
    }
    // Teamkennung neben dem Tor
    g.cuboid(Vec3::new(2.3, 1.6, gz + 0.02), Vec3::new(0.9, 0.9, 0.02), Quat::IDENTITY, p.marking);
    g.cuboid(Vec3::new(2.3, 1.6, gz + 0.035), Vec3::new(0.55, 0.12, 0.01), Quat::IDENTITY, p.paint_dark);
    g.cuboid(Vec3::new(2.3, 1.6, gz + 0.035), Vec3::new(0.12, 0.55, 0.01), Quat::IDENTITY, p.paint_dark);

    // --- Dachaufbauten: Klimageräte mit Lüftern, Rohre
    for (x, z) in [(1.9, bz + 1.3), (-2.2, bz + 1.4)] {
        g.cuboid(Vec3::new(x, 3.35, z), Vec3::new(1.1, 0.45, 0.8), Quat::IDENTITY, p.metal);
        for dx in [-0.25, 0.25] {
            g.cyl(Vec3::new(x + dx, 3.59, z), 0.2, 0.03, Vec3::Y, 12, p.steel_dark);
            g.cuboid(Vec3::new(x + dx, 3.61, z), Vec3::new(0.38, 0.02, 0.04), Quat::IDENTITY, p.metal);
        }
    }
    g.cyl(Vec3::new(0.0, 3.25, bz + 2.05), 0.08, 4.4, Vec3::X, 8, p.metal);

    // --- Leitstand: vorkragende, schräg verglaste Kanzel
    let cz = bz - 0.3;
    let cab = chamfered(4.0, 2.9, 0.6);
    let lower: Vec<Vec3> = cab.iter().map(|v| Vec3::new(v.x * 0.92, 3.12, v.y * 0.92 + cz)).collect();
    let upper: Vec<Vec3> = cab.iter().map(|v| Vec3::new(v.x * 1.06, 3.95, v.y * 1.06 + cz)).collect();
    glass.loft(&lower, &upper, Vec3::ONE);
    // Fensterpfosten
    for v in &cab {
        let a = Vec3::new(v.x * 0.92, 3.12, v.y * 0.92 + cz);
        let b = Vec3::new(v.x * 1.06, 3.95, v.y * 1.06 + cz);
        let mid = (a + b) / 2.0;
        let dir = b - a;
        g.mesh(
            Cuboid::new(0.08, dir.length(), 0.08).mesh().build(),
            Transform::from_translation(mid).with_rotation(Quat::from_rotation_arc(Vec3::Y, dir.normalize())),
            p.steel_dark,
        );
    }
    let roof: Vec<Vec2> = cab.iter().map(|v| *v * 1.1 + Vec2::new(0.0, cz)).collect();
    g.extrude_y(&roof, 3.95, 4.2, 0.96, cz * 0.04, p.paint_dark);
    // Gittermast mit Antennen und Satellitenschüssel
    let mast = Vec3::new(-1.2, 4.2, cz - 0.4);
    for (dx, dz) in [(-0.18, -0.18), (0.18, -0.18), (0.18, 0.18), (-0.18, 0.18)] {
        g.cyl(mast + Vec3::new(dx * 0.6, 1.6, dz * 0.6), 0.025, 3.2, Vec3::new(-dx * 0.08, 1.0, -dz * 0.08), 4, p.metal);
    }
    for k in 0..6 {
        g.cuboid(mast + Vec3::Y * (0.4 + k as f32 * 0.5), Vec3::new(0.3 - k as f32 * 0.03, 0.02, 0.3 - k as f32 * 0.03), Quat::from_rotation_y(0.785 * (k % 2) as f32), p.metal);
    }
    g.cyl(mast + Vec3::Y * 3.9, 0.015, 1.4, Vec3::Y, 4, p.steel_dark);
    glow.mesh(Sphere::new(0.08).mesh().ico(1).expect("Kugel"), Transform::from_translation(mast + Vec3::Y * 4.62), Vec3::ONE);
    let sat = Vec3::new(1.0, 4.2, cz + 0.6);
    g.cyl(sat + Vec3::Y * 0.25, 0.05, 0.5, Vec3::Y, 6, p.metal);
    g.frustum(sat + Vec3::new(0.0, 0.62, -0.05), 0.05, 0.55, 0.25, Vec3::new(0.0, 0.7, -0.7), 16, lin(Color::srgb(0.75, 0.75, 0.73)));

    // --- Radarturm: konischer Betonturm mit zwei Plattformen und Radom
    let tw = Vec3::new(3.0, 0.0, 2.7);
    g.frustum(tw + Vec3::Y * 2.6, 0.75, 0.5, 4.3, Vec3::Y, 16, concrete);
    for (y, r) in [(2.2, 0.95), (4.75, 0.85)] {
        g.cyl(tw + Vec3::Y * y, r, 0.1, Vec3::Y, 16, p.steel_dark);
        ring_rail(&mut g, tw + Vec3::Y * (y + 0.05), r - 0.05, 12, p.metal);
    }
    // Leiter am Turm
    for k in 0..14 {
        g.cuboid(tw + Vec3::new(-0.72 + k as f32 * 0.008, 0.6 + k as f32 * 0.3, 0.0), Vec3::new(0.04, 0.02, 0.3), Quat::IDENTITY, p.metal);
    }
    g.mesh(Sphere::new(0.62).mesh().ico(3).expect("Kugel"), Transform::from_translation(tw + Vec3::Y * 5.15), lin(Color::srgb(0.82, 0.82, 0.8)));
    glow.mesh(Sphere::new(0.1).mesh().ico(1).expect("Kugel"), Transform::from_translation(tw + Vec3::Y * 6.42), Vec3::ONE);

    // --- Treibstofftanks mit Kuppeldach, Ringen, Leiter und Rohrleitung
    for (x, z) in [(-3.4, 2.5), (-2.0, 3.55)] {
        let c = Vec3::new(x, base, z);
        g.cyl(c + Vec3::Y * 0.9, 0.62, 1.8, Vec3::Y, 18, concrete);
        g.mesh(Sphere::new(0.62).mesh().uv(18, 8).scaled_by(Vec3::new(1.0, 0.35, 1.0)), Transform::from_translation(c + Vec3::Y * 1.8), concrete);
        for y in [0.4, 1.0, 1.6] {
            g.cyl(c + Vec3::Y * y, 0.635, 0.06, Vec3::Y, 18, p.paint_dark);
        }
        g.cyl(c + Vec3::new(0.0, 0.25, 0.0), 0.66, 0.12, Vec3::Y, 18, p.marking);
        for k in 0..6 {
            g.cuboid(c + Vec3::new(0.64, 0.2 + k as f32 * 0.3, 0.0), Vec3::new(0.03, 0.02, 0.22), Quat::IDENTITY, p.metal);
        }
    }
    g.cyl(Vec3::new(-2.6, base + 0.5, 1.2), 0.07, 2.6, Vec3::new(0.3, 0.0, 1.0), 8, p.metal);

    // --- Container und Kisten
    for (pos, rot, col) in [
        (Vec3::new(3.7, base, -2.7), 0.0, p.paint_dark),
        (Vec3::new(3.7, base + 1.0, -2.6), 0.12, lin(Color::srgb(0.45, 0.42, 0.33))),
    ] {
        let r = Quat::from_rotation_y(FRAC_PI_2 + rot);
        g.cuboid(pos + Vec3::Y * 0.5, Vec3::new(2.4, 1.0, 1.0), r, col);
        for k in 0..9 {
            let off = r * Vec3::new(-1.1 + k as f32 * 0.275, 0.0, 0.51);
            g.cuboid(pos + Vec3::Y * 0.5 + off, Vec3::new(0.05, 0.92, 0.02), r, col * 0.8);
        }
    }
    for (x, z) in [(-3.9, -3.5), (-3.3, -3.9), (-3.65, -3.65)] {
        let y = if x == -3.65 { base + 0.55 } else { base };
        g.cuboid(Vec3::new(x, y + 0.275, z), Vec3::splat(0.55), Quat::from_rotation_y(x * 3.0), lin(Color::srgb(0.42, 0.34, 0.22)));
    }

    // --- Flutlichtmasten an den Ecken
    for (x, z) in [(-4.4, -4.4), (4.4, -4.4), (-4.4, 4.4), (4.4, 4.4)] {
        let c = Vec3::new(x, base, z);
        let inward = -Vec3::new(x, 0.0, z).normalize();
        g.cyl(c + Vec3::Y * 1.2, 0.06, 2.4, Vec3::Y, 6, p.steel_dark);
        g.cuboid(c + Vec3::Y * 2.4, Vec3::new(0.7, 0.06, 0.06), Quat::from_rotation_y(x.atan2(z)), p.steel_dark);
        for s in [-0.25f32, 0.25] {
            let side = Quat::from_rotation_y(x.atan2(z)) * Vec3::X * s;
            g.cuboid(c + Vec3::Y * 2.5 + side, Vec3::new(0.2, 0.14, 0.12), Quat::from_rotation_y(x.atan2(z)), p.metal);
            glow.cuboid(c + Vec3::Y * 2.48 + side + inward * 0.065, Vec3::new(0.16, 0.08, 0.01), Quat::from_rotation_y(x.atan2(z)), Vec3::ONE);
        }
    }

    // --- Betonsperren an der Rückseite und den Flanken
    let jersey = [Vec2::new(-0.3, 0.0), Vec2::new(0.3, 0.0), Vec2::new(0.12, 0.25), Vec2::new(0.08, 0.6), Vec2::new(-0.08, 0.6), Vec2::new(-0.12, 0.25)];
    for i in 0..5 {
        let x = -3.6 + i as f32 * 1.8;
        let pts: Vec<Vec2> = jersey.iter().map(|v| Vec2::new(v.x - 5.25, v.y)).collect();
        g.extrude_x(&pts, x - 0.8, x + 0.8, concrete);
    }

    g.weather(p.dust, 1.4, 0.45);

    // --- Radarschüssel (eigenes Mesh, dreht sich)
    let mut dish = Geo::default();
    dish.cyl(Vec3::Y * 0.15, 0.04, 0.3, Vec3::Y, 6, p.steel_dark);
    dish.cuboid(Vec3::new(0.0, 0.42, 0.0), Vec3::new(1.5, 0.35, 0.06), Quat::from_rotation_x(-0.25), lin(Color::srgb(0.8, 0.8, 0.78)));
    dish.cuboid(Vec3::new(0.0, 0.42, 0.06), Vec3::new(1.4, 0.05, 0.08), Quat::IDENTITY, p.steel_dark);

    HqMeshes {
        dish_pos: tw + Vec3::Y * 5.72,
        body: meshes.add(g.build().expect("HQ")),
        glow: meshes.add(glow.build().expect("HQ-Licht")),
        glass: meshes.add(glass.build().expect("HQ-Glas")),
        dish: meshes.add(dish.build().expect("Radar")),
    }
}
