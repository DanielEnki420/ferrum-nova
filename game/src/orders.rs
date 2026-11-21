//! Befehle: Wegpunkte, Patrouille, Position halten, Formation, Gruppentempo, Befehlsmarker.
//!
//! - Shift+Rechtsklick hängt Ziele an die Befehlsschlange (`Waypoints`).
//! - Alt+Rechtsklick lässt Einheiten zwischen ihrem Standort und dem Ziel patrouillieren.
//! - Gruppen fahren im Tempo der langsamsten Einheit und stellen sich am Ziel in Formation auf:
//!   Reihen quer zur Fahrtrichtung oder entlang einer mit der rechten Maustaste gezogenen Frontlinie.
//! - Am Boden zeigen kurz aufleuchtende Marker, welcher Befehl wohin ging.

use crate::components::*;
use bevy::prelude::*;

/// Weitere Ziele nach dem laufenden Marschbefehl: (Ziel, als Angriffsmarsch, Blickrichtung)
#[derive(Component, Default, Debug)]
pub struct Waypoints(pub Vec<(Vec3, bool, Option<f32>)>);

/// Pendelt zwischen `a` und `b` (Angriffsmarsch), fährt gerade nach `b`
#[derive(Component, Debug)]
pub struct Patrol {
    pub a: Vec3,
    pub b: Vec3,
}

/// Bleibt stehen und schießt nur auf Ziele in Reichweite
#[derive(Component)]
pub struct HoldPosition;

/// Gemeinsames Tempo einer Gruppe auf dem Marsch
#[derive(Component)]
pub struct GroupSpeed(pub f32);

/// Blickrichtung nach der Ankunft (Formation)
#[derive(Component)]
pub struct FaceOnArrival(pub f32);

/// Sammelpunkt des HQ für neue Einheiten (auf einem Feld: Sammler bauen dort ab)
#[derive(Component)]
pub struct Rally {
    pub pos: Vec3,
    pub node: Option<Entity>,
}

/// Meldungen über Angriffe auf eigene Einheiten und Gebäude
#[derive(Resource, Default)]
pub struct Alerts {
    /// (Ort, Alter in Sekunden) – für die Minikarte
    pub pings: Vec<(Vec3, f32)>,
    /// Ort des letzten Angriffs (Leertaste springt hin)
    pub last: Option<Vec3>,
    pub since: f32,
}

pub const ALERT_SHOW: f32 = 15.0;

impl Alerts {
    /// Treffer melden; nahe beieinander liegende Treffer zählen als ein Angriff
    pub fn hit(&mut self, pos: Vec3) {
        self.last = Some(pos);
        self.since = 0.0;
        if !self.pings.iter().any(|(p, age)| *age < 4.0 && flat(*p).distance(flat(pos)) < 14.0) {
            self.pings.push((pos, 0.0));
        }
    }

    pub fn tick(&mut self, dt: f32) {
        self.since += dt;
        for p in &mut self.pings {
            p.1 += dt;
        }
        self.pings.retain(|p| p.1 < 6.0);
    }

    /// Ziel für die Leertaste, solange der letzte Angriff frisch ist
    pub fn recent(&self) -> Option<Vec3> {
        self.last.filter(|_| self.since < ALERT_SHOW)
    }
}

pub fn tick_alerts(time: Res<Time>, mut alerts: ResMut<Alerts>) {
    alerts.tick(time.delta_secs());
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MarkerKind {
    Move,
    AttackMove,
    Attack,
    Patrol,
    Rally,
}

impl MarkerKind {
    pub fn color(self) -> Color {
        match self {
            MarkerKind::Move => Color::srgb(0.35, 1.0, 0.45),
            MarkerKind::AttackMove => Color::srgb(1.0, 0.6, 0.15),
            MarkerKind::Attack => Color::srgb(1.0, 0.2, 0.2),
            MarkerKind::Patrol => Color::srgb(0.35, 0.7, 1.0),
            MarkerKind::Rally => Color::srgb(1.0, 0.9, 0.3),
        }
    }
}

/// Kurz aufleuchtender Befehlsmarker am Boden (folgt bei Angriffen dem Ziel)
#[derive(Component)]
pub struct Marker {
    pub kind: MarkerKind,
    pub pos: Vec3,
    pub target: Option<Entity>,
    pub age: f32,
}

const MARKER_LIFE: f32 = 0.9;

pub fn spawn_marker(c: &mut Commands, kind: MarkerKind, pos: Vec3, target: Option<Entity>) {
    c.spawn(Marker { kind, pos, target, age: 0.0 });
}

/// Alle laufenden Befehle aufheben (Stopp)
pub fn clear_orders(ec: &mut EntityCommands) {
    ec.remove::<(MoveTo, AttackMove, AttackTarget, Waypoints, Patrol, HoldPosition, GroupSpeed, FaceOnArrival)>();
}

/// Eine Einheit der Auswahl, wie sie für Gruppenbefehle gebraucht wird
#[derive(Clone, Copy, Debug)]
pub struct Member {
    pub e: Entity,
    pub pos: Vec3,
    pub speed: f32,
    pub radius: f32,
    pub armed: bool,
    pub harvester: bool,
}

/// Plätze für `n` Einheiten in Reihen quer zur Blickrichtung `facing`.
/// - `width`: Breite der vordersten Reihe (gezogene Frontlinie), sonst etwa doppelt so breit wie tief
/// - `front`: `center` ist die Mitte der vordersten Reihe statt der Mitte der ganzen Formation
pub fn formation_slots(n: usize, center: Vec3, facing: Vec3, width: Option<f32>, spacing: f32, front: bool) -> Vec<Vec3> {
    if n == 0 {
        return Vec::new();
    }
    let fwd = flat(facing).normalize_or(Vec3::Z);
    let right = Vec3::new(-fwd.z, 0.0, fwd.x);
    let cols = match width {
        Some(w) => ((w / spacing).floor() as usize + 1).clamp(1, n),
        None => ((n as f32 * 2.0).sqrt().ceil() as usize).clamp(1, n),
    };
    let rows = n.div_ceil(cols);
    let mut slots = Vec::with_capacity(n);
    for i in 0..n {
        let row = i / cols;
        let in_row = if row == rows - 1 { n - row * cols } else { cols };
        let col = i % cols;
        // in breiten Frontlinien die Abstände auf die Linie verteilen
        let step = match width {
            Some(w) if in_row > 1 && row == 0 => (w / (in_row - 1) as f32).max(spacing),
            _ => spacing,
        };
        let x = (col as f32 - (in_row as f32 - 1.0) / 2.0) * step;
        let z = -(row as f32) * spacing;
        slots.push(right * x + fwd * z);
    }
    let shift = if front { Vec3::ZERO } else { slots.iter().sum::<Vec3>() / n as f32 };
    slots.into_iter().map(|s| center + s - shift).collect()
}

/// Einheiten so auf Plätze verteilen, dass sich die Wege nicht kreuzen:
/// vorderste Einheiten in die vorderen Reihen, innerhalb einer Reihe von links nach rechts.
/// Ergebnis: für jede Einheit der Index ihres Platzes.
pub fn assign_slots(units: &[Vec3], slots: &[Vec3], facing: Vec3) -> Vec<usize> {
    let fwd = flat(facing).normalize_or(Vec3::Z);
    let right = Vec3::new(-fwd.z, 0.0, fwd.x);
    let key = |p: &Vec3, axis: Vec3| flat(*p).dot(axis);
    // Plätze: Reihen von vorn nach hinten, je Reihe seitlich sortiert
    let mut slot_order: Vec<usize> = (0..slots.len()).collect();
    slot_order.sort_by(|a, b| {
        let fa = (key(&slots[*a], fwd) * 10.0).round();
        let fb = (key(&slots[*b], fwd) * 10.0).round();
        fb.total_cmp(&fa).then(key(&slots[*a], right).total_cmp(&key(&slots[*b], right)))
    });
    // Reihengrößen in dieser Reihenfolge
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut last: Option<f32> = None;
    for &s in &slot_order {
        let f = (key(&slots[s], fwd) * 10.0).round();
        if last != Some(f) {
            rows.push(Vec::new());
            last = Some(f);
        }
        rows.last_mut().unwrap().push(s);
    }
    // Einheiten: die vordersten zuerst, dann je Reihe seitlich sortiert
    let mut unit_order: Vec<usize> = (0..units.len()).collect();
    unit_order.sort_by(|a, b| key(&units[*b], fwd).total_cmp(&key(&units[*a], fwd)));
    let mut result = vec![0; units.len()];
    let mut taken = 0;
    for row in rows {
        let mut chunk: Vec<usize> = unit_order[taken..(taken + row.len()).min(units.len())].to_vec();
        taken += chunk.len();
        chunk.sort_by(|a, b| key(&units[*a], right).total_cmp(&key(&units[*b], right)));
        for (u, s) in chunk.into_iter().zip(row) {
            result[u] = s;
        }
    }
    result
}

/// Art eines Gruppenbefehls an einen Punkt
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoveKind {
    Move,
    AttackMove,
    Patrol,
}

/// Gruppe an einen Punkt schicken: Formation, gemeinsames Tempo, Blickrichtung am Ziel.
/// Läuft in der Simulation (net::cmd::apply_cmds); den Marker setzt die Eingabe selbst.
/// `line`: gezogene Frontlinie (Anfang, Ende). `queue`: an die Befehlsschlange anhängen.
pub fn issue_group_move(c: &mut Commands, members: &[Member], target: Vec3, line: Option<(Vec3, Vec3)>, kind: MoveKind, queue: bool) {
    let movers: Vec<Member> = members.iter().copied().filter(|m| kind == MoveKind::Move || m.armed).collect();
    if movers.is_empty() {
        return;
    }
    let centroid = movers.iter().map(|m| flat(m.pos)).sum::<Vec3>() / movers.len() as f32;
    let spacing = movers.iter().map(|m| m.radius * 2.0 + 0.8).fold(3.0, f32::max);
    let (center, facing, width, front) = match line {
        Some((a, b)) => {
            let mid = (a + b) * 0.5;
            let along = flat(b - a).normalize_or(Vec3::X);
            let mut n = Vec3::new(along.z, 0.0, -along.x);
            // Front zeigt von der Gruppe weg
            if n.dot(flat(mid) - centroid) < 0.0 {
                n = -n;
            }
            (mid, n, Some(flat(b - a).length()), true)
        }
        None => (target, flat(target) - centroid, None, false),
    };
    let slots = formation_slots(movers.len(), center, facing, width, spacing, front);
    let positions: Vec<Vec3> = movers.iter().map(|m| m.pos).collect();
    let assignment = assign_slots(&positions, &slots, facing);
    let heading = Heading::from_dir(flat(facing).normalize_or(Vec3::Z)).0;
    let group_speed = movers.iter().map(|m| m.speed).fold(f32::MAX, f32::min);
    for (m, slot) in movers.iter().zip(assignment) {
        let goal = clamp_map(slots[slot]);
        let mut ec = c.entity(m.e);
        if queue {
            // an die Schlange anhängen; ohne laufenden Befehl wird sie sofort abgearbeitet
            ec.entry::<Waypoints>().or_default().and_modify(move |mut w| w.0.push((goal, kind != MoveKind::Move, Some(heading))));
            continue;
        }
        clear_orders(&mut ec);
        ec.insert((MoveTo(goal), FaceOnArrival(heading)));
        if movers.len() > 1 {
            ec.insert(GroupSpeed(group_speed));
        }
        match kind {
            MoveKind::Move => {}
            MoveKind::AttackMove => {
                ec.insert(AttackMove);
            }
            MoveKind::Patrol => {
                ec.insert((AttackMove, Patrol { a: flat(m.pos), b: goal }));
            }
        }
    }
}

/// Marker eines Gruppenbefehls (nur auf dem Rechner, der den Befehl gibt)
pub fn group_marker(c: &mut Commands, target: Vec3, line: Option<(Vec3, Vec3)>, kind: MoveKind) {
    let marker = match kind {
        MoveKind::Move => MarkerKind::Move,
        MoveKind::AttackMove => MarkerKind::AttackMove,
        MoveKind::Patrol => MarkerKind::Patrol,
    };
    spawn_marker(c, marker, line.map_or(target, |(a, b)| (a + b) * 0.5), None);
}

fn clamp_map(p: Vec3) -> Vec3 {
    let lim = MAP_HALF - 2.0;
    Vec3::new(p.x.clamp(-lim, lim), p.y, p.z.clamp(-lim, lim))
}

/// Befehlsschlange und Patrouille abarbeiten, sobald der laufende Befehl erledigt ist
#[allow(clippy::type_complexity)]
pub fn follow_orders(
    mut commands: Commands,
    mut q: Query<(Entity, &Transform, Option<&mut Waypoints>, Option<&mut Patrol>), (Without<MoveTo>, Without<AttackTarget>)>,
) {
    for (e, t, wp, patrol) in &mut q {
        if let Some(mut wp) = wp {
            if wp.0.is_empty() {
                commands.entity(e).remove::<Waypoints>();
                continue;
            }
            let (goal, attack, face) = wp.0.remove(0);
            let mut ec = commands.entity(e);
            ec.insert(MoveTo(goal)).remove::<HoldPosition>();
            if attack {
                ec.insert(AttackMove);
            } else {
                ec.remove::<AttackMove>();
            }
            if let Some(h) = face {
                ec.insert(FaceOnArrival(h));
            }
            continue;
        }
        if let Some(mut p) = patrol {
            // umkehren, wenn das Ziel erreicht ist
            if flat(t.translation).distance(flat(p.b)) < 2.5 {
                let (a, b) = (p.a, p.b);
                p.a = b;
                p.b = a;
            }
            commands.entity(e).insert((MoveTo(p.b), AttackMove));
        }
    }
}

/// Nach der Ankunft in die Formationsrichtung drehen
pub fn face_on_arrival(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Heading, &FaceOnArrival), (Without<MoveTo>, Without<AttackTarget>)>,
) {
    let dt = time.delta_secs();
    for (e, mut h, f) in &mut q {
        let diff = (f.0 - h.0 + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        if diff.abs() < 0.03 {
            commands.entity(e).remove::<FaceOnArrival>();
            continue;
        }
        h.0 += diff.clamp(-2.5 * dt, 2.5 * dt);
    }
}

/// Befehlsmarker zeichnen: ein Ring, der sich zusammenzieht und verblasst
pub fn draw_markers(mut commands: Commands, time: Res<Time>, mut gizmos: Gizmos, mut q: Query<(Entity, &mut Marker)>, targets: Query<&GlobalTransform>) {
    for (e, mut m) in &mut q {
        m.age += time.delta_secs();
        if m.age > MARKER_LIFE {
            commands.entity(e).despawn();
            continue;
        }
        if let Some(t) = m.target.and_then(|t| targets.get(t).ok()) {
            m.pos = t.translation();
        }
        let k = m.age / MARKER_LIFE;
        let col = m.kind.color().with_alpha(1.0 - k);
        let g = crate::terrain::on_ground(m.pos) + Vec3::Y * 0.2;
        let iso = Isometry3d::new(g, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
        let r = if m.kind == MarkerKind::Attack { 2.2 - 1.0 * k } else { 1.6 - 1.0 * k };
        gizmos.circle(iso, r, col);
        gizmos.circle(iso, r * 0.55, col);
        if m.kind == MarkerKind::Rally {
            gizmos.line(g, g + Vec3::Y * 3.0, col);
            gizmos.line(g + Vec3::Y * 3.0, g + Vec3::new(1.0, 2.6, 0.0), col);
        }
        if m.kind == MarkerKind::Attack {
            // Fadenkreuz
            for a in [0.0f32, std::f32::consts::FRAC_PI_2, std::f32::consts::PI, 1.5 * std::f32::consts::PI] {
                let d = Vec3::new(a.cos(), 0.0, a.sin());
                gizmos.line(g + d * r * 0.7, g + d * r * 1.3, col);
            }
        }
    }
}
