//! Detaillierte Fahrzeugmodelle.
//!
//! Jedes Modell wird einmal pro Typ und Team aus echter Geometrie gebaut (schräge Platten,
//! Kettenumlauf, Laufrollen, Anbauteile) und zu wenigen Meshes zusammengefasst:
//! Wanne, Turm, Leuchtteile und Glas. Die Farben stecken in den Vertexfarben, inklusive
//! Staub an der Unterseite und leichter Fleckigkeit. So bleiben es pro Fahrzeug nur
//! drei bis vier Zeichenaufrufe, egal wie viele Details dazukommen.
//! Vorne ist -Z, der Ursprung liegt auf dem Boden.

use crate::components::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Fertige Meshes eines Fahrzeugs
#[derive(Clone)]
pub struct VehicleMeshes {
    pub hull: Handle<Mesh>,
    pub hull_glow: Option<Handle<Mesh>>,
    pub glass: Option<Handle<Mesh>>,
    pub turret: Option<Handle<Mesh>>,
    pub turret_glow: Option<Handle<Mesh>>,
}

/// Geometrie-Sammler mit Vertexfarben
#[derive(Default)]
struct Geo {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

fn lin(c: Color) -> Vec3 {
    let l = LinearRgba::from(c);
    Vec3::new(l.red, l.green, l.blue)
}

impl Geo {
    fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, col: Vec3) {
        let n = (b - a).cross(c - a).normalize_or_zero();
        let base = self.pos.len() as u32;
        for p in [a, b, c] {
            self.pos.push(p.to_array());
            self.nrm.push(n.to_array());
            self.col.push([col.x, col.y, col.z, 1.0]);
        }
        self.idx.extend_from_slice(&[base, base + 1, base + 2]);
    }

    /// Verbindet zwei gleich lange, konvexe Ringe (Seitenflächen plus Deckel).
    /// Die Flächen werden flach schattiert und zeigen automatisch nach außen.
    fn loft(&mut self, a: &[Vec3], b: &[Vec3], col: Vec3) {
        let n = a.len();
        let center = (a.iter().sum::<Vec3>() + b.iter().sum::<Vec3>()) / (2 * n) as f32;
        let mut face = |pts: &[Vec3]| {
            // Fächer-Triangulierung, Orientierung nach außen
            let fc = pts.iter().sum::<Vec3>() / pts.len() as f32;
            let nrm = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
            let flip = nrm.dot(fc - center) < 0.0;
            for i in 1..pts.len() - 1 {
                if flip {
                    self.tri(pts[0], pts[i + 1], pts[i], col);
                } else {
                    self.tri(pts[0], pts[i], pts[i + 1], col);
                }
            }
        };
        for i in 0..n {
            let j = (i + 1) % n;
            face(&[a[i], a[j], b[j], b[i]]);
        }
        face(a);
        face(b);
    }

    /// Seitenprofil (z, y) entlang x extrudieren
    fn extrude_x(&mut self, profile: &[Vec2], x0: f32, x1: f32, col: Vec3) {
        let a: Vec<Vec3> = profile.iter().map(|p| Vec3::new(x0, p.y, p.x)).collect();
        let b: Vec<Vec3> = profile.iter().map(|p| Vec3::new(x1, p.y, p.x)).collect();
        self.loft(&a, &b, col);
    }

    /// Grundriss (x, z) von y0 bis y1, oben auf `top_scale` verjüngt und um `top_shift` (z) versetzt
    fn extrude_y(&mut self, plan: &[Vec2], y0: f32, y1: f32, top_scale: f32, top_shift: f32, col: Vec3) {
        let a: Vec<Vec3> = plan.iter().map(|p| Vec3::new(p.x, y0, p.y)).collect();
        let b: Vec<Vec3> = plan.iter().map(|p| Vec3::new(p.x * top_scale, y1, p.y * top_scale + top_shift)).collect();
        self.loft(&a, &b, col);
    }

    fn cuboid(&mut self, center: Vec3, size: Vec3, rot: Quat, col: Vec3) {
        self.mesh(Cuboid::from_size(size).mesh().build(), Transform::from_translation(center).with_rotation(rot), col);
    }

    /// Ein Bevy-Primitiv übernehmen
    fn mesh(&mut self, mesh: Mesh, tf: Transform, col: Vec3) {
        let m = mesh.transformed_by(tf);
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else { return };
        let Some(VertexAttributeValues::Float32x3(n)) = m.attribute(Mesh::ATTRIBUTE_NORMAL) else { return };
        let base = self.pos.len() as u32;
        self.pos.extend_from_slice(p);
        self.nrm.extend_from_slice(n);
        self.col.extend(std::iter::repeat_n([col.x, col.y, col.z, 1.0], p.len()));
        match m.indices() {
            Some(Indices::U32(i)) => self.idx.extend(i.iter().map(|i| i + base)),
            Some(Indices::U16(i)) => self.idx.extend(i.iter().map(|&i| i as u32 + base)),
            None => {}
        }
    }

    /// Zylinder entlang einer Achse (Rad, Rohr)
    fn cyl(&mut self, center: Vec3, radius: f32, len: f32, axis: Vec3, res: u32, col: Vec3) {
        let rot = Quat::from_rotation_arc(Vec3::Y, axis.normalize());
        self.mesh(Cylinder::new(radius, len).mesh().resolution(res).build(), Transform::from_translation(center).with_rotation(rot), col);
    }

    fn frustum(&mut self, center: Vec3, r_bottom: f32, r_top: f32, len: f32, axis: Vec3, res: u32, col: Vec3) {
        let rot = Quat::from_rotation_arc(Vec3::Y, axis.normalize());
        let m = ConicalFrustum { radius_top: r_top, radius_bottom: r_bottom, height: len }.mesh().resolution(res).build();
        self.mesh(m, Transform::from_translation(center).with_rotation(rot), col);
    }

    /// Staub nach unten hin, leichte Fleckigkeit, Kanten etwas aufgehellt
    fn weather(&mut self, dust: Vec3, dust_top: f32, amount: f32) {
        for (p, c) in self.pos.iter().zip(self.col.iter_mut()) {
            let v = Vec3::from_array(*p);
            let h = ((v.x * 12.9898 + v.y * 78.233 + v.z * 37.719).sin() * 43_758.547).fract().abs();
            let mut col = Vec3::new(c[0], c[1], c[2]);
            col *= 0.9 + 0.14 * h;
            let k = if dust_top > 0.0 { (1.0 - (v.y / dust_top).clamp(0.0, 1.0)).powf(1.5) * amount } else { 0.0 };
            col = col.lerp(dust, k);
            *c = [col.x, col.y, col.z, 1.0];
        }
    }

    fn build(self) -> Option<Mesh> {
        if self.idx.is_empty() {
            return None;
        }
        Some(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
                .with_inserted_indices(Indices::U32(self.idx)),
        )
    }
}

/// Farbpalette eines Teams
struct Palette {
    paint: Vec3,
    paint_dark: Vec3,
    marking: Vec3,
    metal: Vec3,
    steel_dark: Vec3,
    rubber: Vec3,
    dust: Vec3,
}

fn palette(team: Team) -> Palette {
    let (paint, dark, mark) = if team == PLAYER {
        (Color::srgb(0.40, 0.47, 0.50), Color::srgb(0.25, 0.29, 0.31), Color::srgb(0.55, 0.78, 0.95))
    } else {
        (Color::srgb(0.47, 0.30, 0.24), Color::srgb(0.29, 0.18, 0.15), Color::srgb(0.95, 0.45, 0.25))
    };
    Palette {
        paint: lin(paint),
        paint_dark: lin(dark),
        marking: lin(mark),
        metal: lin(Color::srgb(0.30, 0.30, 0.29)),
        steel_dark: lin(Color::srgb(0.13, 0.13, 0.13)),
        rubber: lin(Color::srgb(0.07, 0.07, 0.065)),
        dust: lin(Color::srgb(0.50, 0.44, 0.34)),
    }
}

/// Stadionförmiger Kettenumlauf mit Laufrollen, Antriebsrad, Leitrad und Stützrollen
fn track_unit(g: &mut Geo, p: &Palette, side: f32, x: f32, len: f32, wheels: usize, skirt: bool) {
    let w = 0.40;
    let r_front = 0.30;
    let r_rear = 0.27;
    let zf = -len / 2.0 + r_front;
    let zr = len / 2.0 - r_rear;
    let yf = 0.42;
    let yr = 0.34;
    // Außenkontur der Kette (z, y): vorne höher gezogen, unten flach
    let mut prof = Vec::new();
    for i in 0..=8 {
        let a = PI * 0.5 + PI * i as f32 / 8.0; // vorn: oben -> unten
        prof.push(Vec2::new(zf + a.cos() * r_front, yf + a.sin() * r_front));
    }
    for i in 0..=8 {
        let a = PI * 1.5 + PI * i as f32 / 8.0; // hinten: unten -> oben
        prof.push(Vec2::new(zr + a.cos() * r_rear, yr + a.sin() * r_rear));
    }
    // flache Unterseite ein Stück anheben, damit die Kette auf dem Boden liegt
    for v in prof.iter_mut() {
        v.y = v.y.max(0.03);
    }
    g.extrude_x(&prof, x - w / 2.0, x + w / 2.0, p.rubber);

    // Greifer (Querstollen) auf der Lauffläche unten
    let n = (len / 0.16) as usize;
    for i in 0..n {
        let z = zf + (zr - zf) * i as f32 / (n - 1) as f32;
        g.cuboid(Vec3::new(x, 0.035, z), Vec3::new(w + 0.02, 0.05, 0.05), Quat::IDENTITY, p.steel_dark);
    }
    // Rollen an der Außenseite
    let ox = x + side * (w / 2.0 + 0.012);
    for i in 0..wheels {
        let z = zf + 0.12 + (zr - zf - 0.24) * i as f32 / (wheels - 1) as f32;
        g.cyl(Vec3::new(ox, 0.27, z), 0.21, 0.06, Vec3::X, 14, p.metal);
        g.cyl(Vec3::new(ox + side * 0.035, 0.27, z), 0.08, 0.03, Vec3::X, 8, p.steel_dark);
    }
    // Antriebsrad vorn (mit Zähnen), Leitrad hinten, Stützrollen oben
    g.cyl(Vec3::new(ox, yf, zf), 0.24, 0.07, Vec3::X, 12, p.steel_dark);
    for k in 0..10 {
        let a = TAU * k as f32 / 10.0;
        g.cuboid(
            Vec3::new(ox, yf + a.sin() * 0.25, zf + a.cos() * 0.25),
            Vec3::new(0.07, 0.06, 0.06),
            Quat::from_rotation_x(a),
            p.steel_dark,
        );
    }
    g.cyl(Vec3::new(ox, yr, zr), 0.2, 0.07, Vec3::X, 12, p.metal);
    for k in 0..3 {
        let z = zf + 0.4 + (zr - zf - 0.8) * k as f32 / 2.0;
        g.cyl(Vec3::new(ox - side * 0.02, 0.6, z), 0.06, 0.05, Vec3::X, 8, p.metal);
    }
    // Kettenschürzen in Segmenten mit Fugen
    if skirt {
        let segs = 4;
        let seg_len = (len - 0.3) / segs as f32;
        for k in 0..segs {
            let z = -len / 2.0 + 0.2 + seg_len * (k as f32 + 0.5);
            g.cuboid(
                Vec3::new(x + side * (w / 2.0 + 0.06), 0.58, z),
                Vec3::new(0.035, 0.24, seg_len - 0.03),
                Quat::from_rotation_z(side * 0.08),
                p.paint,
            );
        }
    }
}

/// Wanne eines Kettenfahrzeugs: Seitenprofil mit schrägem Bug und Heck, Kotflügel, Motordeck
fn tracked_hull(g: &mut Geo, p: &Palette, len: f32, deck_y: f32) {
    let h = len / 2.0;
    let prof = [
        Vec2::new(-h + 0.05, 0.32),
        Vec2::new(-h - 0.08, 0.56),
        Vec2::new(-h + 0.55, deck_y),
        Vec2::new(h - 0.12, deck_y),
        Vec2::new(h, deck_y - 0.12),
        Vec2::new(h - 0.05, 0.32),
    ];
    g.extrude_x(&prof, -0.52, 0.52, p.paint);
    // Kotflügel über den Ketten
    for side in [-1.0f32, 1.0] {
        g.cuboid(Vec3::new(side * 0.74, 0.72, 0.0), Vec3::new(0.5, 0.04, len + 0.1), Quat::IDENTITY, p.paint);
        // Staukästen auf den Kotflügeln
        g.cuboid(Vec3::new(side * 0.78, 0.8, h - 0.55), Vec3::new(0.34, 0.14, 0.5), Quat::IDENTITY, p.paint_dark);
    }
    // Motordeck: Lüftungsgitter aus Lamellen
    for k in 0..6 {
        let z = h - 0.75 + k as f32 * 0.09;
        g.cuboid(Vec3::new(0.0, deck_y + 0.015, z), Vec3::new(0.7, 0.025, 0.045), Quat::IDENTITY, p.steel_dark);
    }
    // Auspuffe hinten, Abschlepphaken, Scheinwerfergehäuse vorn
    for side in [-1.0f32, 1.0] {
        g.cyl(Vec3::new(side * 0.32, deck_y - 0.16, h + 0.02), 0.06, 0.12, Vec3::Z, 8, p.steel_dark);
        g.cuboid(Vec3::new(side * 0.35, 0.36, -h - 0.02), Vec3::new(0.08, 0.08, 0.12), Quat::IDENTITY, p.steel_dark);
        g.cyl(Vec3::new(side * 0.42, 0.64, -h + 0.18), 0.06, 0.08, Vec3::Z, 10, p.metal);
    }
    // Ersatzkettenglieder auf dem Bugblech
    for k in 0..4 {
        let x = -0.27 + k as f32 * 0.18;
        g.cuboid(
            Vec3::new(x, 0.66, -h + 0.22),
            Vec3::new(0.15, 0.03, 0.16),
            Quat::from_rotation_x(-0.75),
            p.steel_dark,
        );
    }
    // Fahrerluke
    g.cyl(Vec3::new(-0.25, deck_y + 0.02, -h + 0.75), 0.15, 0.05, Vec3::Y, 12, p.paint_dark);
}

fn finish(g: Geo, meshes: &mut Assets<Mesh>) -> Option<Handle<Mesh>> {
    g.build().map(|m| meshes.add(m))
}

pub fn build_panzer(team: Team, meshes: &mut Assets<Mesh>) -> VehicleMeshes {
    let p = palette(team);
    let len = 2.9;
    let mut hull = Geo::default();
    for side in [-1.0f32, 1.0] {
        track_unit(&mut hull, &p, side, side * 0.72, len, 6, true);
    }
    tracked_hull(&mut hull, &p, len, 0.84);
    hull.weather(p.dust, 0.9, 0.55);

    let mut hglow = Geo::default();
    for side in [-1.0f32, 1.0] {
        hglow.cyl(Vec3::new(side * 0.42, 0.64, -len / 2.0 + 0.13), 0.045, 0.02, Vec3::Z, 10, Vec3::ONE);
    }

    // Turm: keilförmiger Grundriss mit schrägen Wänden. Ursprung = Drehpunkt (y 0.86, z 0.1 in der Wanne)
    let mut tur = Geo::default();
    let plan = [
        Vec2::new(-0.42, -0.85),
        Vec2::new(0.42, -0.85),
        Vec2::new(0.72, -0.25),
        Vec2::new(0.74, 0.55),
        Vec2::new(0.55, 0.85),
        Vec2::new(-0.55, 0.85),
        Vec2::new(-0.74, 0.55),
        Vec2::new(-0.72, -0.25),
    ];
    tur.extrude_y(&plan, 0.0, 0.40, 0.84, 0.06, p.paint);
    // Kennungsband ums Turmheck (Teamfarbe)
    tur.cuboid(Vec3::new(0.0, 0.2, 0.86), Vec3::new(1.0, 0.07, 0.02), Quat::IDENTITY, p.marking);
    for side in [-1.0f32, 1.0] {
        tur.cuboid(Vec3::new(side * 0.735, 0.2, 0.15), Vec3::new(0.02, 0.07, 0.6), Quat::IDENTITY, p.marking);
    }
    // Blende und Rohr mit Rauchabsauger und Mündungsbremse
    tur.cuboid(Vec3::new(0.0, 0.18, -0.9), Vec3::new(0.38, 0.26, 0.2), Quat::IDENTITY, p.paint_dark);
    tur.frustum(Vec3::new(0.0, 0.17, -1.6), 0.085, 0.06, 1.3, -Vec3::Z, 12, p.metal);
    tur.cyl(Vec3::new(0.0, 0.17, -1.55), 0.095, 0.3, Vec3::Z, 12, p.paint_dark);
    tur.cyl(Vec3::new(0.0, 0.17, -2.2), 0.06, 0.12, Vec3::Z, 10, p.steel_dark);
    tur.cuboid(Vec3::new(0.0, 0.17, -2.32), Vec3::new(0.2, 0.1, 0.14), Quat::IDENTITY, p.steel_dark);
    // Kommandantenkuppel mit Winkelspiegeln, Ladeschützenluke
    tur.cyl(Vec3::new(0.28, 0.46, 0.2), 0.19, 0.12, Vec3::Y, 14, p.paint);
    tur.cyl(Vec3::new(0.28, 0.53, 0.2), 0.15, 0.03, Vec3::Y, 14, p.paint_dark);
    for k in 0..6 {
        let a = TAU * k as f32 / 6.0;
        tur.cuboid(
            Vec3::new(0.28 + a.cos() * 0.17, 0.49, 0.2 + a.sin() * 0.17),
            Vec3::new(0.05, 0.04, 0.05),
            Quat::from_rotation_y(-a),
            p.steel_dark,
        );
    }
    tur.cyl(Vec3::new(-0.28, 0.41, 0.25), 0.15, 0.03, Vec3::Y, 12, p.paint_dark);
    // Zielgerät, Nebelwurfbecher, Antenne, Staukorb am Heck
    tur.cuboid(Vec3::new(-0.3, 0.47, -0.35), Vec3::new(0.18, 0.14, 0.22), Quat::IDENTITY, p.paint_dark);
    for side in [-1.0f32, 1.0] {
        for k in 0..3 {
            tur.cyl(
                Vec3::new(side * (0.62 - k as f32 * 0.05), 0.33 + k as f32 * 0.035, -0.45 + k as f32 * 0.04),
                0.035,
                0.14,
                Vec3::new(side * 0.6, 0.6, -1.0),
                8,
                p.steel_dark,
            );
        }
    }
    tur.cyl(Vec3::new(-0.45, 0.9, 0.6), 0.012, 1.0, Vec3::Y, 5, p.steel_dark);
    tur.cuboid(Vec3::new(0.0, 0.25, 1.02), Vec3::new(1.0, 0.03, 0.3), Quat::IDENTITY, p.steel_dark);
    for x in [-0.48, 0.0, 0.48] {
        tur.cuboid(Vec3::new(x, 0.33, 1.15), Vec3::new(0.03, 0.18, 0.03), Quat::IDENTITY, p.steel_dark);
    }
    tur.cuboid(Vec3::new(0.0, 0.42, 1.15), Vec3::new(1.0, 0.03, 0.03), Quat::IDENTITY, p.steel_dark);
    tur.cuboid(Vec3::new(0.05, 0.36, 1.05), Vec3::new(0.7, 0.16, 0.2), Quat::IDENTITY, p.paint_dark);
    tur.weather(p.dust, 0.0, 0.0);

    let mut tglow = Geo::default();
    tglow.cuboid(Vec3::new(-0.3, 0.47, -0.465), Vec3::new(0.12, 0.07, 0.01), Quat::IDENTITY, Vec3::ONE);

    VehicleMeshes {
        hull: finish(hull, meshes).expect("Wanne"),
        hull_glow: finish(hglow, meshes),
        glass: None,
        turret: finish(tur, meshes),
        turret_glow: finish(tglow, meshes),
    }
}

pub fn build_werfer(team: Team, meshes: &mut Assets<Mesh>) -> VehicleMeshes {
    let p = palette(team);
    let len = 2.5;
    let mut hull = Geo::default();
    for side in [-1.0f32, 1.0] {
        track_unit(&mut hull, &p, side, side * 0.72, len, 5, false);
    }
    tracked_hull(&mut hull, &p, len, 0.86);
    // Fahrerhaus vorn
    let cab = [
        Vec2::new(-1.05, 0.86),
        Vec2::new(-0.95, 1.15),
        Vec2::new(-0.55, 1.2),
        Vec2::new(-0.45, 0.86),
    ];
    hull.extrude_x(&cab, -0.5, 0.5, p.paint_dark);
    // Drehkranz für den Werfer
    hull.cyl(Vec3::new(0.0, 0.92, 0.25), 0.45, 0.12, Vec3::Y, 16, p.steel_dark);
    // Stützen hinten
    for side in [-1.0f32, 1.0] {
        hull.cuboid(Vec3::new(side * 0.55, 0.55, 1.2), Vec3::new(0.08, 0.5, 0.08), Quat::from_rotation_x(0.3), p.steel_dark);
    }
    hull.weather(p.dust, 0.9, 0.55);

    let mut glass = Geo::default();
    glass.cuboid(Vec3::new(0.0, 1.06, -0.99), Vec3::new(0.8, 0.16, 0.02), Quat::from_rotation_x(-0.33), Vec3::ONE);

    let mut hglow = Geo::default();
    hglow.cuboid(Vec3::new(0.0, 1.21, -0.75), Vec3::new(0.5, 0.03, 0.06), Quat::IDENTITY, Vec3::ONE);

    // Werfer: Ursprung = Drehpunkt (y 1.05, z 0.25), wird um `tilt` angehoben
    let mut tur = Geo::default();
    // Lafette mit seitlichen Wangen
    for side in [-1.0f32, 1.0] {
        let cheek = [Vec2::new(-0.2, -0.05), Vec2::new(0.45, -0.05), Vec2::new(0.35, 0.35), Vec2::new(-0.05, 0.3)];
        let x = side * 0.62;
        tur.extrude_x(&cheek, x - 0.04, x + 0.04, p.steel_dark);
    }
    // Rohrbündel 3 x 2 in einem Kasten
    tur.cuboid(Vec3::new(0.0, 0.32, 0.0), Vec3::new(1.12, 0.62, 1.45), Quat::IDENTITY, p.paint);
    tur.cuboid(Vec3::new(0.0, 0.32, -0.74), Vec3::new(1.16, 0.66, 0.05), Quat::IDENTITY, p.paint_dark);
    for xi in [-0.34, 0.0, 0.34] {
        for yi in [0.17, 0.47] {
            tur.cyl(Vec3::new(xi, yi, -0.77), 0.13, 0.05, Vec3::Z, 12, p.metal);
            tur.cyl(Vec3::new(xi, yi, -0.79), 0.1, 0.02, Vec3::Z, 12, p.steel_dark);
        }
    }
    // Versteifungsrippen und Kennungsband
    for z in [-0.4, 0.0, 0.4] {
        tur.cuboid(Vec3::new(0.0, 0.64, z), Vec3::new(1.14, 0.03, 0.06), Quat::IDENTITY, p.paint_dark);
    }
    for side in [-1.0f32, 1.0] {
        tur.cuboid(Vec3::new(side * 0.565, 0.32, 0.2), Vec3::new(0.02, 0.1, 0.8), Quat::IDENTITY, p.marking);
    }
    tur.weather(p.dust, 0.0, 0.0);
    // Raketenspitzen leuchten in der Teamfarbe
    let mut tglow = Geo::default();
    for xi in [-0.34, 0.0, 0.34] {
        for yi in [0.17, 0.47] {
            tglow.mesh(
                Cone::new(0.07, 0.14).mesh().resolution(10).build(),
                Transform::from_xyz(xi, yi, -0.76).with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                Vec3::ONE,
            );
        }
    }

    VehicleMeshes {
        hull: finish(hull, meshes).expect("Wanne"),
        hull_glow: finish(hglow, meshes),
        glass: finish(glass, meshes),
        turret: finish(tur, meshes),
        turret_glow: finish(tglow, meshes),
    }
}

pub fn build_sammler(team: Team, meshes: &mut Assets<Mesh>) -> VehicleMeshes {
    let p = palette(team);
    let mut hull = Geo::default();
    // Rahmen und drei Achsen mit profilierten Reifen
    hull.cuboid(Vec3::new(0.0, 0.5, 0.05), Vec3::new(0.9, 0.18, 2.7), Quat::IDENTITY, p.steel_dark);
    for z in [-0.85, 0.35, 1.05] {
        hull.cyl(Vec3::new(0.0, 0.34, z), 0.06, 1.5, Vec3::X, 8, p.steel_dark);
        for side in [-1.0f32, 1.0] {
            let x = side * 0.72;
            hull.cyl(Vec3::new(x, 0.34, z), 0.34, 0.28, Vec3::X, 18, p.rubber);
            // Profilstollen
            for k in 0..14 {
                let a = TAU * k as f32 / 14.0;
                hull.cuboid(
                    Vec3::new(x, 0.34 + a.sin() * 0.34, z + a.cos() * 0.34),
                    Vec3::new(0.26, 0.045, 0.07),
                    Quat::from_rotation_x(-a),
                    p.rubber,
                );
            }
            hull.cyl(Vec3::new(x + side * 0.14, 0.34, z), 0.18, 0.02, Vec3::X, 12, p.metal);
            hull.cyl(Vec3::new(x + side * 0.155, 0.34, z), 0.06, 0.02, Vec3::X, 8, p.steel_dark);
        }
    }
    // Kotflügel
    for side in [-1.0f32, 1.0] {
        for (z, l) in [(-0.85, 0.85), (0.7, 1.55)] {
            hull.cuboid(Vec3::new(side * 0.72, 0.74, z), Vec3::new(0.36, 0.04, l), Quat::IDENTITY, p.paint_dark);
        }
    }
    // Fahrerhaus mit schräger Frontscheibe
    let cab = [
        Vec2::new(-1.3, 0.6),
        Vec2::new(-1.32, 0.95),
        Vec2::new(-1.12, 1.35),
        Vec2::new(-0.5, 1.38),
        Vec2::new(-0.45, 0.6),
    ];
    hull.extrude_x(&cab, -0.6, 0.6, p.paint);
    hull.cuboid(Vec3::new(0.0, 1.42, -0.85), Vec3::new(1.0, 0.06, 0.55), Quat::IDENTITY, p.paint_dark);
    // Kühlergrill und Stoßstange
    for k in 0..5 {
        hull.cuboid(Vec3::new(0.0, 0.68 + k as f32 * 0.05, -1.33), Vec3::new(0.7, 0.025, 0.02), Quat::IDENTITY, p.steel_dark);
    }
    hull.cuboid(Vec3::new(0.0, 0.55, -1.4), Vec3::new(1.3, 0.12, 0.1), Quat::IDENTITY, p.steel_dark);
    // Erzbunker: unten schmal, oben breit, mit Kante in Teamfarbe
    let hopper_a = [Vec2::new(-0.5, -0.3), Vec2::new(0.5, -0.3), Vec2::new(0.5, 1.3), Vec2::new(-0.5, 1.3)];
    let a: Vec<Vec3> = hopper_a.iter().map(|v| Vec3::new(v.x, 0.62, v.y)).collect();
    let b: Vec<Vec3> = hopper_a.iter().map(|v| Vec3::new(v.x * 1.32, 1.28, v.y * 1.04 + 0.02)).collect();
    hull.loft(&a, &b, p.paint_dark);
    hull.cuboid(Vec3::new(0.0, 1.3, 0.52), Vec3::new(1.36, 0.05, 1.72), Quat::IDENTITY, p.marking);
    // Ladung (Erz) sichtbar im Bunker
    hull.mesh(
        Sphere::new(0.5).mesh().ico(1).expect("Kugel").scaled_by(Vec3::new(1.15, 0.28, 1.4)),
        Transform::from_xyz(0.0, 1.24, 0.52),
        lin(Color::srgb(0.24, 0.22, 0.2)),
    );
    // Greifarm hinten, Auspuff, Leiter
    hull.cuboid(Vec3::new(0.45, 1.25, 1.3), Vec3::new(0.12, 0.12, 0.9), Quat::from_rotation_x(0.6), p.steel_dark);
    hull.cuboid(Vec3::new(0.45, 1.62, 1.0), Vec3::new(0.1, 0.1, 0.7), Quat::from_rotation_x(-0.4), p.metal);
    hull.cyl(Vec3::new(-0.52, 1.35, -0.42), 0.05, 0.75, Vec3::Y, 8, p.steel_dark);
    for k in 0..4 {
        hull.cuboid(Vec3::new(0.62, 0.68 + k as f32 * 0.15, -0.6), Vec3::new(0.02, 0.02, 0.18), Quat::IDENTITY, p.metal);
    }
    hull.weather(p.dust, 1.1, 0.6);

    let mut glass = Geo::default();
    glass.cuboid(Vec3::new(0.0, 1.16, -1.22), Vec3::new(1.05, 0.34, 0.02), Quat::from_rotation_x(0.46), Vec3::ONE);
    for side in [-1.0f32, 1.0] {
        glass.cuboid(Vec3::new(side * 0.605, 1.12, -0.82), Vec3::new(0.02, 0.26, 0.5), Quat::IDENTITY, Vec3::ONE);
    }

    let mut glow = Geo::default();
    for side in [-1.0f32, 1.0] {
        glow.cyl(Vec3::new(side * 0.45, 0.85, -1.34), 0.07, 0.02, Vec3::Z, 10, Vec3::ONE);
    }
    glow.cuboid(Vec3::new(0.0, 1.47, -0.85), Vec3::new(0.6, 0.05, 0.08), Quat::IDENTITY, Vec3::ONE);

    VehicleMeshes {
        hull: finish(hull, meshes).expect("Wanne"),
        hull_glow: finish(glow, meshes),
        glass: finish(glass, meshes),
        turret: None,
        turret_glow: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loft_faces_point_outward() {
        let mut g = Geo::default();
        let plan = [
            Vec2::new(-0.42, -0.85),
            Vec2::new(0.42, -0.85),
            Vec2::new(0.72, -0.25),
            Vec2::new(0.74, 0.55),
            Vec2::new(0.55, 0.85),
            Vec2::new(-0.55, 0.85),
            Vec2::new(-0.74, 0.55),
            Vec2::new(-0.72, -0.25),
        ];
        g.extrude_y(&plan, 0.0, 0.40, 0.84, 0.06, Vec3::ONE);
        let c = Vec3::new(0.0, 0.2, 0.03);
        for t in g.idx.chunks(3) {
            let p: Vec<Vec3> = t.iter().map(|&i| Vec3::from_array(g.pos[i as usize])).collect();
            let n = Vec3::from_array(g.nrm[t[0] as usize]);
            let geo = (p[1] - p[0]).cross(p[2] - p[0]);
            let fc = (p[0] + p[1] + p[2]) / 3.0;
            assert!(n.dot(fc - c) > 0.0, "Normale zeigt nach innen bei {fc}");
            assert!(geo.dot(n) > 0.0, "Wicklung passt nicht zur Normale bei {fc}");
        }
    }
}
