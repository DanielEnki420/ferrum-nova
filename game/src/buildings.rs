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

// ================================================================ Baubare Gebäude

/// Fertige Meshes eines baubaren Gebäudes
#[derive(Clone)]
pub struct StructMeshes {
    pub body: Handle<Mesh>,
    pub glow: Handle<Mesh>,
    pub glass: Option<Handle<Mesh>>,
    /// Drehbarer Geschützkopf: Mesh, Leuchtteile, Drehpunkt
    pub head: Option<(Handle<Mesh>, Handle<Mesh>, Vec3)>,
    /// Rotierendes Bauteil (Lüfter) und seine Lage
    pub spin: Option<(Handle<Mesh>, Vec3)>,
    /// So tief steckt das Gebäude zu Baubeginn im Boden
    pub height: f32,
}

/// Quader zwischen zwei Punkten (Träger, Streben)
fn beam(g: &mut Geo, a: Vec3, b: Vec3, t: f32, col: Vec3) {
    let d = b - a;
    g.mesh(
        Cuboid::new(t, d.length(), t).mesh().build(),
        Transform::from_translation((a + b) / 2.0).with_rotation(Quat::from_rotation_arc(Vec3::Y, d.normalize())),
        col,
    );
}

/// Drehung um die Hochachse, die +X auf die Richtung (x, z) legt
fn yaw_to(d: Vec2) -> Quat {
    Quat::from_rotation_y((-d.y).atan2(d.x))
}

/// Betonfundament mit Fugen. Reicht tief in den Boden, damit es auch am Hang nicht schwebt.
fn foundation(g: &mut Geo, w: f32, c: f32, top: f32) {
    let concrete = lin(Color::srgb(0.50, 0.49, 0.46));
    let joint = lin(Color::srgb(0.34, 0.33, 0.31));
    g.extrude_y(&chamfered(w, w, c), -1.2, top, 0.985, 0.0, concrete);
    let n = (w / 3.8) as i32;
    for k in -n..=n {
        let o = k as f32 * 1.9;
        g.cuboid(Vec3::new(o, top + 0.005, 0.0), Vec3::new(0.04, 0.01, w * 0.9), Quat::IDENTITY, joint);
        g.cuboid(Vec3::new(0.0, top + 0.005, o), Vec3::new(w * 0.9, 0.01, 0.04), Quat::IDENTITY, joint);
    }
}

/// Lampe an einem Mast
fn lamp_post(g: &mut Geo, glow: &mut Geo, base: Vec3, h: f32, toward: Vec3, col: Vec3) {
    g.cyl(base + Vec3::Y * h / 2.0, 0.05, h, Vec3::Y, 6, col);
    let arm = toward.normalize_or_zero() * 0.3;
    beam(g, base + Vec3::Y * h, base + Vec3::Y * h + arm, 0.05, col);
    g.cuboid(base + Vec3::Y * (h - 0.04) + arm, Vec3::new(0.18, 0.1, 0.18), Quat::IDENTITY, col);
    glow.cuboid(base + Vec3::Y * (h - 0.1) + arm, Vec3::new(0.13, 0.01, 0.13), Quat::IDENTITY, Vec3::ONE);
}

/// Geschützturm: Betonsockel mit Brüstung, gepanzerter Drehkranz, Kopf mit Zwillingsrohren
pub fn build_turret(team: Team, meshes: &mut Assets<Mesh>) -> StructMeshes {
    let p = palette(team);
    let concrete = lin(Color::srgb(0.50, 0.49, 0.46));
    let mut g = Geo::default();
    let mut glow = Geo::default();
    let top = 0.4;

    foundation(&mut g, 3.8, 1.0, top);
    // Brüstung aus Betonelementen, hinten offen
    let ring = chamfered(3.5, 3.5, 0.95);
    for i in 0..8 {
        if i == 4 {
            continue;
        }
        let (a, b) = (ring[i] * 0.97, ring[(i + 1) % 8] * 0.97);
        let d = b - a;
        let mid = (a + b) / 2.0;
        g.cuboid(Vec3::new(mid.x, top + 0.28, mid.y), Vec3::new(d.length() + 0.05, 0.56, 0.26), yaw_to(d), concrete);
        g.cuboid(Vec3::new(mid.x, top + 0.57, mid.y), Vec3::new(d.length() - 0.1, 0.04, 0.3), yaw_to(d), lin(Color::srgb(0.40, 0.39, 0.37)));
    }
    hazard(&mut g, Vec3::new(-0.7, top + 0.01, 1.45), 1.4, 6);

    // Achteckiger, gepanzerter Sockel mit Schraubenkranz und Wartungsluke
    g.frustum(Vec3::Y * (top + 0.6), 1.0, 0.8, 1.2, Vec3::Y, 8, p.paint_dark);
    g.cyl(Vec3::Y * (top + 0.06), 1.06, 0.12, Vec3::Y, 8, p.steel_dark);
    for i in 0..16 {
        let a = TAU * i as f32 / 16.0;
        g.cuboid(Vec3::new(a.cos() * 0.76, top + 1.21, a.sin() * 0.76), Vec3::splat(0.05), Quat::IDENTITY, p.metal);
    }
    g.cyl(Vec3::Y * (top + 1.25), 0.86, 0.1, Vec3::Y, 24, p.steel_dark);
    g.cuboid(Vec3::new(0.0, top + 0.55, 0.9), Vec3::new(0.5, 0.65, 0.05), Quat::from_rotation_x(-0.17), p.metal);
    g.cuboid(Vec3::new(0.18, top + 0.55, 0.95), Vec3::new(0.05, 0.12, 0.05), Quat::IDENTITY, p.steel_dark);
    // Munitionskisten und Kabel hinten
    for (x, z, r) in [(-1.0, 1.0, 0.2), (-0.8, 1.25, -0.3)] {
        g.cuboid(Vec3::new(x, top + 0.18, z), Vec3::new(0.5, 0.36, 0.3), Quat::from_rotation_y(r), lin(Color::srgb(0.30, 0.33, 0.22)));
    }
    g.cyl(Vec3::new(0.6, top + 0.05, 1.1), 0.04, 1.0, Vec3::new(0.3, 0.0, 1.0), 6, p.rubber);
    lamp_post(&mut g, &mut glow, Vec3::new(1.35, top, 1.35), 1.4, Vec3::new(-1.0, 0.0, -1.0), p.steel_dark);
    g.weather(p.dust, 1.0, 0.45);

    // --- Kopf (Drehpunkt auf dem Drehkranz, Rohre zeigen nach -Z)
    let mut h = Geo::default();
    let mut hg = Geo::default();
    let plan = [
        Vec2::new(-0.5, -0.95),
        Vec2::new(0.5, -0.95),
        Vec2::new(0.85, -0.35),
        Vec2::new(0.85, 0.85),
        Vec2::new(-0.85, 0.85),
        Vec2::new(-0.85, -0.35),
    ];
    h.extrude_y(&plan, 0.0, 0.8, 0.86, 0.05, p.paint);
    let roof: Vec<Vec2> = plan.iter().map(|v| *v * 0.86 + Vec2::new(0.0, 0.05)).collect();
    h.extrude_y(&roof, 0.8, 0.86, 0.94, 0.0, p.paint_dark);
    // Zusatzpanzer an den Seiten, Blende vorne
    for s in [-1.0f32, 1.0] {
        h.cuboid(Vec3::new(s * 0.9, 0.42, 0.2), Vec3::new(0.07, 0.62, 1.15), Quat::from_rotation_z(s * 0.12), p.paint_dark);
        h.cuboid(Vec3::new(s * 0.955, 0.45, 0.35), Vec3::new(0.01, 0.24, 0.36), Quat::from_rotation_z(s * 0.12), p.marking);
        for k in 0..3 {
            h.cuboid(Vec3::new(s * 0.94, 0.68, -0.15 + k as f32 * 0.4), Vec3::splat(0.05), Quat::IDENTITY, p.metal);
        }
    }
    h.cuboid(Vec3::new(0.0, 0.4, -1.0), Vec3::new(0.95, 0.52, 0.2), Quat::IDENTITY, p.paint_dark);
    // Zwillingsrohre mit Rauchabsauger und Mündungsbremse
    for x in [-0.25f32, 0.25] {
        h.frustum(Vec3::new(x, 0.4, -1.28), 0.14, 0.1, 0.36, Vec3::NEG_Z, 10, p.steel_dark);
        h.cyl(Vec3::new(x, 0.4, -2.2), 0.07, 1.9, Vec3::Z, 10, p.steel_dark);
        h.cyl(Vec3::new(x, 0.4, -1.95), 0.095, 0.3, Vec3::Z, 10, p.metal);
        h.cuboid(Vec3::new(x, 0.4, -3.18), Vec3::new(0.2, 0.13, 0.24), Quat::IDENTITY, p.steel_dark);
        h.cuboid(Vec3::new(x, 0.4, -3.18), Vec3::new(0.22, 0.05, 0.06), Quat::IDENTITY, p.metal);
    }
    // Sensorkasten mit Linse, Luke, Antenne
    h.cuboid(Vec3::new(0.45, 1.02, -0.15), Vec3::new(0.34, 0.3, 0.42), Quat::IDENTITY, p.metal);
    h.cuboid(Vec3::new(0.45, 1.19, -0.15), Vec3::new(0.38, 0.04, 0.46), Quat::IDENTITY, p.steel_dark);
    hg.cuboid(Vec3::new(0.45, 1.02, -0.365), Vec3::new(0.2, 0.1, 0.01), Quat::IDENTITY, Vec3::ONE);
    h.cyl(Vec3::new(-0.32, 0.89, 0.3), 0.24, 0.06, Vec3::Y, 14, p.paint_dark);
    h.cuboid(Vec3::new(-0.32, 0.93, 0.3), Vec3::new(0.3, 0.03, 0.05), Quat::IDENTITY, p.metal);
    h.cyl(Vec3::new(-0.62, 1.5, 0.68), 0.012, 1.4, Vec3::Y, 4, p.steel_dark);
    // Munitionskasten hinten mit Leuchtstreifen
    h.cuboid(Vec3::new(0.0, 0.45, 0.98), Vec3::new(1.3, 0.55, 0.32), Quat::IDENTITY, p.paint_dark);
    for x in [-0.4f32, 0.0, 0.4] {
        h.cuboid(Vec3::new(x, 0.45, 1.145), Vec3::new(0.3, 0.4, 0.02), Quat::IDENTITY, p.paint);
    }
    hg.cuboid(Vec3::new(0.0, 0.67, 1.15), Vec3::new(0.9, 0.04, 0.01), Quat::IDENTITY, Vec3::ONE);
    h.weather(p.dust, 0.0, 0.0);

    StructMeshes {
        body: meshes.add(g.build().expect("Turm")),
        glow: meshes.add(glow.build().expect("Turm-Licht")),
        glass: None,
        head: Some((meshes.add(h.build().expect("Kopf")), meshes.add(hg.build().expect("Kopf-Licht")), Vec3::Y * (top + 1.3))),
        spin: None,
        height: 3.4,
    }
}

/// Raffinerie: Halle mit Satteldach, zwei Silos auf Stelzen, Abladebucht mit Förderband, Schornstein.
/// Die Bucht liegt auf +Z.
pub fn build_refinery(team: Team, meshes: &mut Assets<Mesh>) -> StructMeshes {
    let p = palette(team);
    let concrete = lin(Color::srgb(0.50, 0.49, 0.46));
    let mut g = Geo::default();
    let mut glow = Geo::default();
    let top = 0.35;
    foundation(&mut g, 6.8, 0.9, top);

    // --- Halle
    let off = Vec2::new(-1.3, -1.0);
    let hall: Vec<Vec2> = chamfered(3.4, 3.8, 0.3).into_iter().map(|v| v + off).collect();
    g.extrude_y(&hall, top, 2.5, 1.0, 0.0, p.paint);
    let plinth: Vec<Vec2> = chamfered(3.52, 3.92, 0.34).into_iter().map(|v| v + off).collect();
    g.extrude_y(&plinth, top, top + 0.4, 1.0, 0.0, p.paint_dark);
    // Satteldach mit First entlang x, Dachrinne
    g.extrude_x(&[Vec2::new(-3.0, 2.5), Vec2::new(1.0, 2.5), Vec2::new(-1.0, 3.2)], -3.12, 0.52, p.paint_dark);
    for z in [-3.0f32, 1.0] {
        g.cyl(Vec3::new(-1.3, 2.47, z), 0.06, 3.7, Vec3::X, 6, p.metal);
    }
    // Pfeiler an den Längsseiten
    for z in [-2.6f32, -1.6, -0.6, 0.4] {
        for x in [-3.03f32, 0.43] {
            g.cuboid(Vec3::new(x, 1.45, z), Vec3::new(0.12, 2.1, 0.2), Quat::IDENTITY, p.paint_dark);
        }
    }
    glow.cuboid(Vec3::new(-1.3, 1.95, -1.0), Vec3::new(3.42, 0.16, 3.82), Quat::IDENTITY, Vec3::ONE);
    g.cuboid(Vec3::new(-1.3, 2.1, -1.0), Vec3::new(3.6, 0.04, 4.0), Quat::IDENTITY, p.steel_dark);
    // Rolltor zur Bucht
    g.cuboid(Vec3::new(-1.6, top + 0.8, 0.92), Vec3::new(1.5, 1.6, 0.04), Quat::IDENTITY, p.steel_dark);
    for k in 0..7 {
        g.cuboid(Vec3::new(-1.6, top + 0.12 + k as f32 * 0.21, 0.95), Vec3::new(1.4, 0.15, 0.03), Quat::IDENTITY, p.metal);
    }
    for x in [-2.4f32, -0.8] {
        g.cuboid(Vec3::new(x, top + 0.85, 0.97), Vec3::new(0.1, 1.7, 0.08), Quat::IDENTITY, lin(Color::srgb(0.78, 0.62, 0.12)));
    }
    glow.cuboid(Vec3::new(-1.6, top + 1.82, 0.99), Vec3::new(0.9, 0.05, 0.02), Quat::IDENTITY, Vec3::ONE);
    // Teamkennung an der Seitenwand
    g.cuboid(Vec3::new(-3.02, 1.35, -1.6), Vec3::new(0.02, 0.8, 0.8), Quat::IDENTITY, p.marking);
    g.cuboid(Vec3::new(-3.03, 1.35, -1.6), Vec3::new(0.02, 0.12, 0.5), Quat::IDENTITY, p.paint_dark);
    // Lüftungsgitter auf dem Dach
    for x in [-2.3f32, -0.3] {
        g.cuboid(Vec3::new(x, 3.0, -1.0), Vec3::new(0.5, 0.25, 0.5), Quat::IDENTITY, p.metal);
        g.cuboid(Vec3::new(x, 3.15, -1.0), Vec3::new(0.6, 0.04, 0.6), Quat::IDENTITY, p.steel_dark);
    }

    // --- Silos auf Stelzen mit Trichter, Ringen und Leiter
    for z in [-2.05f32, -0.35] {
        let c = Vec3::new(1.95, top, z);
        for (dx, dz) in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
            g.cyl(c + Vec3::new(dx, 0.7, dz), 0.06, 1.4, Vec3::Y, 6, p.steel_dark);
        }
        beam(&mut g, c + Vec3::new(-0.5, 0.1, -0.5), c + Vec3::new(0.5, 1.2, -0.5), 0.04, p.steel_dark);
        beam(&mut g, c + Vec3::new(0.5, 0.1, 0.5), c + Vec3::new(-0.5, 1.2, 0.5), 0.04, p.steel_dark);
        g.frustum(c + Vec3::Y * 1.0, 0.16, 0.78, 0.8, Vec3::Y, 16, p.metal);
        g.cyl(c + Vec3::Y * 2.6, 0.78, 2.4, Vec3::Y, 20, p.paint);
        g.frustum(c + Vec3::Y * 4.1, 0.78, 0.18, 0.6, Vec3::Y, 20, p.paint_dark);
        for y in [1.55f32, 2.4, 3.2, 3.75] {
            g.cyl(c + Vec3::Y * y, 0.8, 0.05, Vec3::Y, 20, p.paint_dark);
        }
        g.cyl(c + Vec3::Y * 2.8, 0.79, 0.22, Vec3::Y, 20, p.marking);
        for k in 0..10 {
            g.cuboid(c + Vec3::new(0.81, 1.5 + k as f32 * 0.25, 0.0), Vec3::new(0.03, 0.02, 0.26), Quat::IDENTITY, p.metal);
        }
        glow.cyl(c + Vec3::Y * 4.45, 0.06, 0.08, Vec3::Y, 8, Vec3::ONE);
        g.cyl(c + Vec3::Y * 0.45, 0.08, 0.6, Vec3::Y, 8, p.steel_dark);
    }
    // Laufsteg zwischen den Silos, Rohr zur Halle
    beam(&mut g, Vec3::new(1.95, top + 4.0, -2.05), Vec3::new(1.95, top + 4.0, -0.35), 0.16, p.steel_dark);
    g.cyl(Vec3::new(0.85, 1.8, -1.2), 0.1, 0.9, Vec3::X, 8, p.metal);
    g.cyl(Vec3::new(0.85, top + 0.15, -2.05), 0.08, 1.0, Vec3::X, 8, p.metal);

    // --- Abladebucht: Stahlrost mit Warnstreifen, Erzhaufen, Förderband in die Halle
    g.cuboid(Vec3::new(0.0, top + 0.01, 2.3), Vec3::new(2.8, 0.02, 1.6), Quat::IDENTITY, p.steel_dark);
    for k in 0..12 {
        g.cuboid(Vec3::new(-1.3 + k as f32 * 0.236, top + 0.03, 2.3), Vec3::new(0.04, 0.02, 1.5), Quat::IDENTITY, p.metal);
    }
    hazard(&mut g, Vec3::new(-1.4, top + 0.02, 3.2), 2.8, 10);
    hazard(&mut g, Vec3::new(-1.4, top + 0.02, 1.4), 2.8, 10);
    g.mesh(
        Sphere::new(0.6).mesh().ico(1).expect("Haufen").scaled_by(Vec3::new(1.3, 0.45, 1.0)),
        Transform::from_translation(Vec3::new(-0.7, top + 0.05, 2.35)),
        lin(Color::srgb(0.30, 0.26, 0.21)),
    );
    let (a, b) = (Vec3::new(0.6, top + 0.35, 2.4), Vec3::new(0.6, 2.6, 0.3));
    let d = b - a;
    let belt_rot = Quat::from_rotation_arc(Vec3::Z, d.normalize());
    g.mesh(Cuboid::new(0.6, 0.1, d.length()).mesh().build(), Transform::from_translation((a + b) / 2.0).with_rotation(belt_rot), p.rubber);
    for s in [-0.34f32, 0.34] {
        beam(&mut g, a + Vec3::new(s, 0.1, 0.0), b + Vec3::new(s, 0.1, 0.0), 0.06, p.paint_dark);
    }
    for k in 1..4 {
        let m = a + d * (k as f32 / 4.0);
        g.cyl(Vec3::new(m.x, (m.y + top) / 2.0, m.z), 0.05, m.y - top, Vec3::Y, 6, p.steel_dark);
    }
    g.cuboid(b + Vec3::new(0.0, 0.25, -0.1), Vec3::new(0.8, 0.6, 0.5), Quat::IDENTITY, p.metal);

    // --- Schornstein durchs Dach mit Ringen und Warnlicht
    let ch = Vec3::new(-2.55, 0.0, -2.45);
    g.cyl(ch + Vec3::Y * 2.9, 0.24, 5.1, Vec3::Y, 14, concrete);
    for (y, col) in [(4.3, p.paint_dark), (4.9, p.marking), (5.4, p.paint_dark)] {
        g.cyl(ch + Vec3::Y * y, 0.26, 0.12, Vec3::Y, 14, col);
    }
    glow.cyl(ch + Vec3::Y * 5.48, 0.2, 0.04, Vec3::Y, 14, Vec3::ONE);

    lamp_post(&mut g, &mut glow, Vec3::new(1.6, top, 3.0), 2.0, Vec3::new(-1.0, 0.0, -0.3), p.steel_dark);
    for (x, z) in [(-2.9f32, 2.6f32), (-2.5, 2.9)] {
        g.cyl(Vec3::new(x, top + 0.42, z), 0.28, 0.84, Vec3::Y, 12, lin(Color::srgb(0.35, 0.30, 0.18)));
    }
    g.weather(p.dust, 1.3, 0.45);

    StructMeshes {
        body: meshes.add(g.build().expect("Raffinerie")),
        glow: meshes.add(glow.build().expect("Raffinerie-Licht")),
        glass: None,
        head: None,
        spin: None,
        height: 5.8,
    }
}

/// Fabrik: Halle mit Sheddach und Oberlichtern, Hallentor auf +Z, Schornsteine,
/// Kühlaggregat mit drehendem Lüfter und Portalkran
pub fn build_factory(team: Team, meshes: &mut Assets<Mesh>) -> StructMeshes {
    let p = palette(team);
    let concrete = lin(Color::srgb(0.50, 0.49, 0.46));
    let mut g = Geo::default();
    let mut glow = Geo::default();
    let mut glass = Geo::default();
    let top = 0.35;
    foundation(&mut g, 8.0, 1.0, top);

    // --- Halle
    let (cx, cz, w, d) = (-0.5f32, -0.45f32, 5.6f32, 5.3f32);
    let off = Vec2::new(cx, cz);
    let hall: Vec<Vec2> = chamfered(w, d, 0.3).into_iter().map(|v| v + off).collect();
    g.extrude_y(&hall, top, 3.0, 1.0, 0.0, p.paint);
    let plinth: Vec<Vec2> = chamfered(w + 0.12, d + 0.12, 0.34).into_iter().map(|v| v + off).collect();
    g.extrude_y(&plinth, top, top + 0.45, 1.0, 0.0, p.paint_dark);
    // Pfeiler
    for k in 0..6 {
        let x = cx - w / 2.0 + 0.4 + k as f32 * (w - 0.8) / 5.0;
        g.cuboid(Vec3::new(x, 1.7, cz - d / 2.0 - 0.04), Vec3::new(0.2, 2.6, 0.12), Quat::IDENTITY, p.paint_dark);
    }
    for k in 0..5 {
        let z = cz - d / 2.0 + 0.4 + k as f32 * (d - 0.8) / 4.0;
        for x in [cx - w / 2.0 - 0.04, cx + w / 2.0 + 0.04] {
            g.cuboid(Vec3::new(x, 1.7, z), Vec3::new(0.12, 2.6, 0.2), Quat::IDENTITY, p.paint_dark);
        }
    }
    glow.cuboid(Vec3::new(cx, 2.35, cz), Vec3::new(w + 0.02, 0.15, d + 0.02), Quat::IDENTITY, Vec3::ONE);
    g.cuboid(Vec3::new(cx, 2.5, cz), Vec3::new(w + 0.2, 0.04, d + 0.2), Quat::IDENTITY, p.steel_dark);
    // Sheddach: drei Zähne, die steile Seite verglast (nach -Z)
    let teeth = 3;
    let tw = d / teeth as f32;
    for i in 0..teeth {
        let z0 = cz - d / 2.0 + i as f32 * tw;
        g.extrude_x(&[Vec2::new(z0, 3.0), Vec2::new(z0 + tw, 3.0), Vec2::new(z0, 3.85)], cx - w / 2.0 - 0.05, cx + w / 2.0 + 0.05, p.paint_dark);
        glass.cuboid(Vec3::new(cx, 3.42, z0 - 0.02), Vec3::new(w - 0.1, 0.74, 0.03), Quat::IDENTITY, Vec3::ONE);
        for k in 0..8 {
            let x = cx - w / 2.0 + 0.3 + k as f32 * (w - 0.6) / 7.0;
            g.cuboid(Vec3::new(x, 3.42, z0 - 0.04), Vec3::new(0.05, 0.8, 0.04), Quat::IDENTITY, p.steel_dark);
        }
    }

    // --- Hallentor mit Lamellen, Rahmen, Warnstreifen und Lampen
    let gz = cz + d / 2.0 + 0.02;
    let gw = 3.2;
    g.cuboid(Vec3::new(cx, top + 1.2, gz), Vec3::new(gw, 2.4, 0.04), Quat::IDENTITY, p.steel_dark);
    for k in 0..11 {
        g.cuboid(Vec3::new(cx, top + 0.12 + k as f32 * 0.215, gz + 0.03), Vec3::new(gw - 0.15, 0.16, 0.03), Quat::IDENTITY, p.metal);
    }
    for s in [-1.0f32, 1.0] {
        g.cuboid(Vec3::new(cx + s * (gw / 2.0 + 0.08), top + 1.25, gz + 0.05), Vec3::new(0.16, 2.5, 0.12), Quat::IDENTITY, p.paint_dark);
        g.cuboid(Vec3::new(cx + s * (gw / 2.0 + 0.08), top + 0.6, gz + 0.12), Vec3::new(0.17, 1.2, 0.02), Quat::IDENTITY, lin(Color::srgb(0.78, 0.62, 0.12)));
    }
    g.cuboid(Vec3::new(cx, top + 2.52, gz + 0.06), Vec3::new(gw + 0.4, 0.16, 0.14), Quat::IDENTITY, p.paint_dark);
    for k in 0..4 {
        let x = cx - 1.2 + k as f32 * 0.8;
        g.cuboid(Vec3::new(x, top + 2.66, gz + 0.12), Vec3::new(0.24, 0.08, 0.16), Quat::IDENTITY, p.steel_dark);
        glow.cuboid(Vec3::new(x, top + 2.61, gz + 0.2), Vec3::new(0.18, 0.03, 0.01), Quat::IDENTITY, Vec3::ONE);
    }
    hazard(&mut g, Vec3::new(cx - gw / 2.0, top + 0.01, gz + 0.5), gw, 12);
    // Teamkennung neben dem Tor
    g.cuboid(Vec3::new(cx + 2.25, 1.7, gz + 0.01), Vec3::new(0.75, 0.75, 0.02), Quat::IDENTITY, p.marking);
    g.cuboid(Vec3::new(cx + 2.25, 1.7, gz + 0.025), Vec3::new(0.45, 0.1, 0.01), Quat::IDENTITY, p.paint_dark);
    g.cuboid(Vec3::new(cx + 2.25, 1.7, gz + 0.025), Vec3::new(0.1, 0.45, 0.01), Quat::IDENTITY, p.paint_dark);

    // --- Schornsteine durchs Dach
    for (x, h) in [(0.9f32, 6.2f32), (1.6, 5.6)] {
        let c = Vec3::new(x, 0.0, cz - d / 2.0 + 0.55);
        g.cyl(c + Vec3::Y * (h / 2.0 + 0.5), 0.22, h - 1.0, Vec3::Y, 14, concrete);
        for (y, col) in [(h - 0.9, p.paint_dark), (h - 0.45, p.marking), (h - 0.05, p.paint_dark)] {
            g.cyl(c + Vec3::Y * y, 0.24, 0.12, Vec3::Y, 14, col);
        }
        glow.cyl(c + Vec3::Y * (h + 0.02), 0.18, 0.04, Vec3::Y, 14, Vec3::ONE);
    }

    // --- Kühlaggregat an der Ostseite mit Gitter und Lüfter
    let ac = Vec3::new(cx + w / 2.0 + 0.75, top, cz - 1.0);
    g.cuboid(ac + Vec3::Y * 0.6, Vec3::new(1.1, 1.2, 1.7), Quat::IDENTITY, p.metal);
    for k in 0..7 {
        g.cuboid(ac + Vec3::new(0.56, 0.6, -0.7 + k as f32 * 0.233), Vec3::new(0.02, 1.0, 0.05), Quat::IDENTITY, p.steel_dark);
    }
    g.cyl(ac + Vec3::new(0.0, 1.26, 0.0), 0.5, 0.12, Vec3::Y, 20, p.steel_dark);
    g.cyl(ac + Vec3::new(-0.3, 1.0, 0.95), 0.08, 0.7, Vec3::new(-1.0, 0.0, 0.0), 8, p.metal);

    // --- Portalkran über dem Lagerplatz an der Westseite
    let kx = cx - w / 2.0 - 0.85;
    for z in [cz - 1.6, cz + 1.4] {
        beam(&mut g, Vec3::new(kx - 0.45, top, z), Vec3::new(kx, top + 3.4, z), 0.12, lin(Color::srgb(0.78, 0.62, 0.12)));
        beam(&mut g, Vec3::new(kx + 0.45, top, z), Vec3::new(kx, top + 3.4, z), 0.12, lin(Color::srgb(0.78, 0.62, 0.12)));
    }
    beam(&mut g, Vec3::new(kx, top + 3.45, cz - 1.8), Vec3::new(kx, top + 3.45, cz + 1.6), 0.2, lin(Color::srgb(0.78, 0.62, 0.12)));
    g.cuboid(Vec3::new(kx, top + 3.25, cz - 0.2), Vec3::new(0.35, 0.25, 0.4), Quat::IDENTITY, p.steel_dark);
    g.cyl(Vec3::new(kx, top + 2.5, cz - 0.2), 0.015, 1.3, Vec3::Y, 4, p.steel_dark);
    g.cuboid(Vec3::new(kx, top + 1.82, cz - 0.2), Vec3::new(0.18, 0.14, 0.12), Quat::IDENTITY, lin(Color::srgb(0.78, 0.62, 0.12)));
    // Lager: Stahlträger und Kisten
    for k in 0..3 {
        g.cuboid(Vec3::new(kx, top + 0.1 + k as f32 * 0.2, cz + 0.8), Vec3::new(0.6, 0.18, 1.6), Quat::from_rotation_y(k as f32 * 0.05), p.metal);
    }
    for (z, s) in [(cz - 1.0, 0.6f32), (cz - 1.4, 0.5)] {
        g.cuboid(Vec3::new(kx, top + s / 2.0, z), Vec3::splat(s), Quat::from_rotation_y(z), lin(Color::srgb(0.42, 0.34, 0.22)));
    }
    lamp_post(&mut g, &mut glow, Vec3::new(cx + 2.3, top, gz + 1.0), 2.3, Vec3::new(-1.0, 0.0, -0.4), p.steel_dark);
    g.weather(p.dust, 1.5, 0.45);

    // Lüfterrad
    let mut fan = Geo::default();
    fan.cyl(Vec3::ZERO, 0.09, 0.1, Vec3::Y, 10, p.steel_dark);
    for i in 0..5 {
        let a = TAU * i as f32 / 5.0;
        fan.cuboid(
            Vec3::new(a.cos() * 0.24, 0.0, a.sin() * 0.24),
            Vec3::new(0.34, 0.02, 0.12),
            Quat::from_rotation_y(-a) * Quat::from_rotation_x(0.35),
            p.metal,
        );
    }

    StructMeshes {
        body: meshes.add(g.build().expect("Fabrik")),
        glow: meshes.add(glow.build().expect("Fabrik-Licht")),
        glass: Some(meshes.add(glass.build().expect("Fabrik-Glas"))),
        head: None,
        spin: Some((meshes.add(fan.build().expect("Lüfter")), ac + Vec3::Y * 1.3)),
        height: 6.6,
    }
}
