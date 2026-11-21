//! Wegfindung: ein Raster über die Karte (1 m) und A* mit Umwegen um Steilhänge, Gebäude und Mauern.
//!
//! Für jedes Team gibt es ein Abstandsfeld: wie weit jede Zelle vom nächsten Hindernis entfernt ist.
//! Eine Einheit passt durch eine Zelle, wenn dieser Abstand zu ihrem Radius passt. So kommen schmale
//! Fahrzeuge durch Lücken, die ein Bollwerk umfahren muss. Eigene Tore sind für das eigene Team frei.
//!
//! Dazu gibt es je Team ein Durchbruchsfeld, in dem feindliche Mauern fehlen, ihre Zellen aber teuer
//! sind. Ist ein Ziel nur durch eine Mauer erreichbar, greifen die Einheiten das Mauerstück an,
//! das auf dem günstigsten Weg liegt: Sie brechen an der dünnsten Stelle durch.
//!
//! Wege werden nicht sofort berechnet, sondern über eine Warteschlange mit einem Knotenbudget pro
//! Frame. Bis dahin fährt eine Einheit geradeaus.

use crate::components::*;
use crate::terrain;
use bevy::prelude::*;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet, VecDeque};

/// Zellengröße und Zellen je Kante
pub const CELL: f32 = 1.0;
pub const N: usize = (2.0 * MAP_HALF / CELL) as usize;
/// Hänge steiler als etwa 40° sind unbefahrbar
const MAX_SLOPE_NORMAL_Y: f32 = 0.76;
/// Aufschlag je Zelle, in der eine feindliche Mauer steht (Durchbruchssuche)
const WALL_COST: f32 = 20.0;
/// Untersuchte Knoten pro Frame für alle Wegsuchen zusammen
const NODE_BUDGET: i32 = 40_000;
/// So weit um eine Mauer herum gelten Zellen als "Mauer im Weg"
const WALL_REACH: f32 = 2.2;

/// Eine Einheit passt durch eine Zelle mit diesem Mindestabstand zum Hindernis
pub fn need(radius: f32) -> f32 {
    radius * 0.9
}

fn cell_of(p: Vec3) -> (usize, usize) {
    let f = |v: f32| (((v + MAP_HALF) / CELL).floor() as i32).clamp(0, N as i32 - 1) as usize;
    (f(p.x), f(p.z))
}

fn idx(p: Vec3) -> usize {
    let (x, z) = cell_of(p);
    z * N + x
}

pub fn center(i: usize) -> Vec3 {
    let (x, z) = (i % N, i / N);
    Vec3::new(-MAP_HALF + (x as f32 + 0.5) * CELL, 0.0, -MAP_HALF + (z as f32 + 0.5) * CELL)
}

/// Hindernis für das Raster
#[derive(Clone, Copy)]
pub struct Obstacle {
    pub pos: Vec3,
    pub radius: f32,
    pub team: Team,
    pub entity: Entity,
    pub kind: ObstacleKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ObstacleKind {
    Building,
    Wall,
    Gate,
}

#[derive(Resource)]
pub struct NavGrid {
    terrain: Vec<bool>,
    /// Abstand zum nächsten Hindernis (m), je Team
    clear: [Vec<f32>; 2],
    /// Wie `clear`, aber ohne feindliche Mauern und Tore
    breach: [Vec<f32>; 2],
    /// Mauerstück, das eine Zelle (mit etwas Rand) belegt
    wall_at: Vec<Option<(Entity, Team)>>,
    /// Zählt bei jeder Änderung hoch; Wege aus älteren Versionen werden neu berechnet
    pub version: u32,
}

impl Default for NavGrid {
    fn default() -> Self {
        let mut terrain = vec![false; N * N];
        for (i, t) in terrain.iter_mut().enumerate() {
            let (x, z) = (i % N, i / N);
            let c = center(i);
            let edge = x == 0 || z == 0 || x == N - 1 || z == N - 1;
            *t = edge || terrain::normal(c.x, c.z).y < MAX_SLOPE_NORMAL_Y;
        }
        let mut g = Self {
            terrain,
            clear: [Vec::new(), Vec::new()],
            breach: [Vec::new(), Vec::new()],
            wall_at: vec![None; N * N],
            version: 0,
        };
        g.rebuild(&[]);
        g
    }
}

/// Abstandstransformation (Chamfer 1 / √2): 0 auf Hindernissen, sonst Abstand zum nächsten
fn distance_field(blocked: &[bool]) -> Vec<f32> {
    const D: f32 = std::f32::consts::SQRT_2;
    let mut d: Vec<f32> = blocked.iter().map(|b| if *b { 0.0 } else { 1e6 }).collect();
    let n = N as i32;
    let at = |x: i32, z: i32| (z * n + x) as usize;
    for z in 0..n {
        for x in 0..n {
            let mut v = d[at(x, z)];
            for (dx, dz, c) in [(-1, 0, 1.0), (0, -1, 1.0), (-1, -1, D), (1, -1, D)] {
                let (nx, nz) = (x + dx, z + dz);
                if nx >= 0 && nz >= 0 && nx < n {
                    v = v.min(d[at(nx, nz)] + c);
                }
            }
            d[at(x, z)] = v;
        }
    }
    for z in (0..n).rev() {
        for x in (0..n).rev() {
            let mut v = d[at(x, z)];
            for (dx, dz, c) in [(1, 0, 1.0), (0, 1, 1.0), (1, 1, D), (-1, 1, D)] {
                let (nx, nz) = (x + dx, z + dz);
                if nx >= 0 && nx < n && nz < n {
                    v = v.min(d[at(nx, nz)] + c);
                }
            }
            d[at(x, z)] = v;
        }
    }
    // Abstand von Zellmitte zu Zellrand des Hindernisses
    d.iter().map(|v| if *v == 0.0 { 0.0 } else { (v - 0.5) * CELL }).collect()
}

fn stamp(blocked: &mut [bool], pos: Vec3, radius: f32) {
    let r = radius / CELL + 1.0;
    let (cx, cz) = cell_of(pos);
    let r_i = r.ceil() as i32;
    for dz in -r_i..=r_i {
        for dx in -r_i..=r_i {
            let (x, z) = (cx as i32 + dx, cz as i32 + dz);
            if x < 0 || z < 0 || x >= N as i32 || z >= N as i32 {
                continue;
            }
            let i = z as usize * N + x as usize;
            if flat(center(i)).distance(flat(pos)) <= radius {
                blocked[i] = true;
            }
        }
    }
}

impl NavGrid {
    /// Raster aus Gelände und Hindernissen neu aufbauen
    pub fn rebuild(&mut self, obstacles: &[Obstacle]) {
        self.wall_at.iter_mut().for_each(|w| *w = None);
        for o in obstacles.iter().filter(|o| o.kind != ObstacleKind::Building) {
            let (cx, cz) = cell_of(o.pos);
            let r = ((o.radius + WALL_REACH) / CELL).ceil() as i32;
            for dz in -r..=r {
                for dx in -r..=r {
                    let (x, z) = (cx as i32 + dx, cz as i32 + dz);
                    if x < 0 || z < 0 || x >= N as i32 || z >= N as i32 {
                        continue;
                    }
                    let i = z as usize * N + x as usize;
                    if flat(center(i)).distance(flat(o.pos)) <= o.radius + WALL_REACH {
                        self.wall_at[i] = Some((o.entity, o.team));
                    }
                }
            }
        }
        for t in [PLAYER, ENEMY] {
            let ti = t.0 as usize;
            let mut normal = self.terrain.clone();
            let mut breach = self.terrain.clone();
            for o in obstacles {
                let enemy_barrier = o.kind != ObstacleKind::Building && o.team != t;
                // eigene Tore sind offen, fremde Tore und alle Mauern sperren
                let blocks = !(o.kind == ObstacleKind::Gate && o.team == t);
                if blocks {
                    stamp(&mut normal, o.pos, o.radius);
                    if !enemy_barrier {
                        stamp(&mut breach, o.pos, o.radius);
                    }
                }
            }
            self.clear[ti] = distance_field(&normal);
            self.breach[ti] = distance_field(&breach);
        }
        self.version = self.version.wrapping_add(1);
    }

    /// Freier Abstand an einer Stelle für ein Team
    pub fn clearance(&self, team: Team, p: Vec3) -> f32 {
        self.clear[team.0 as usize][idx(p)]
    }

    /// Freie Sicht für eine Einheit dieser Größe auf gerader Strecke
    pub fn line_clear(&self, team: Team, radius: f32, a: Vec3, b: Vec3, breach: bool) -> bool {
        let field = if breach { &self.breach[team.0 as usize] } else { &self.clear[team.0 as usize] };
        let need = need(radius);
        let d = flat(b - a).length();
        let steps = (d / (CELL * 0.35)).ceil().max(1.0) as usize;
        (0..=steps).all(|k| field[idx(a.lerp(b, k as f32 / steps as f32))] >= need)
    }

    /// Nächste befahrbare Zelle um `p` (bis 8 m)
    fn nearest_free(&self, field: &[f32], need: f32, p: Vec3) -> Option<usize> {
        let (cx, cz) = cell_of(p);
        let mut best: Option<(usize, i32)> = None;
        for r in 0..8i32 {
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dz.abs() != r {
                        continue;
                    }
                    let (x, z) = (cx as i32 + dx, cz as i32 + dz);
                    if x < 0 || z < 0 || x >= N as i32 || z >= N as i32 {
                        continue;
                    }
                    let i = z as usize * N + x as usize;
                    let d2 = dx * dx + dz * dz;
                    if field[i] >= need && best.is_none_or(|(_, bd)| d2 < bd) {
                        best = Some((i, d2));
                    }
                }
            }
            if best.is_some() {
                break;
            }
        }
        best.map(|(i, _)| i)
    }
}

/// Ergebnis einer Wegsuche
pub struct Route {
    pub corners: Vec<Vec3>,
    /// Ziel erreicht (sonst endet der Weg am nächstgelegenen erreichbaren Punkt)
    pub reached: bool,
    /// Durchbruchssuche: erstes feindliches Mauerstück auf dem Weg
    pub wall: Option<Entity>,
}

#[derive(PartialEq)]
struct Node(f32, u32);
impl Eq for Node {}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Node {
    fn cmp(&self, o: &Self) -> Ordering {
        // BinaryHeap ist ein Max-Heap: kleinste Kosten zuerst
        o.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal)
    }
}

/// Wiederverwendbarer Speicher der Suche
pub struct Search {
    g: Vec<f32>,
    parent: Vec<u32>,
    seen: Vec<u32>,
    closed: Vec<u32>,
    generation: u32,
    heap: BinaryHeap<Node>,
}

impl Default for Search {
    fn default() -> Self {
        Self { g: vec![0.0; N * N], parent: vec![0; N * N], seen: vec![0; N * N], closed: vec![0; N * N], generation: 0, heap: BinaryHeap::new() }
    }
}

fn octile(a: usize, b: usize) -> f32 {
    let (ax, az, bx, bz) = ((a % N) as f32, (a / N) as f32, (b % N) as f32, (b / N) as f32);
    let (dx, dz) = ((ax - bx).abs(), (az - bz).abs());
    (dx.max(dz) + (std::f32::consts::SQRT_2 - 1.0) * dx.min(dz)) * CELL
}

impl NavGrid {
    /// A* von `from` nach `to` für eine Einheit mit `radius`. `budget` wird um die untersuchten Knoten verringert.
    pub fn find(&self, s: &mut Search, team: Team, radius: f32, from: Vec3, to: Vec3, breach: bool, budget: &mut i32) -> Option<Route> {
        let ti = team.0 as usize;
        let field = if breach { &self.breach[ti] } else { &self.clear[ti] };
        let need = need(radius);
        // Gerade Strecke frei? Dann ohne Suche.
        if !breach && self.line_clear(team, radius, from, to, false) {
            return Some(Route { corners: vec![to], reached: true, wall: None });
        }
        let start = if field[idx(from)] >= need { idx(from) } else { self.nearest_free(field, need, from)? };
        let goal = idx(to);

        s.generation = s.generation.wrapping_add(1).max(1);
        let stamp_id = s.generation;
        s.heap.clear();
        s.g[start] = 0.0;
        s.seen[start] = stamp_id;
        s.parent[start] = start as u32;
        s.heap.push(Node(octile(start, goal), start as u32));
        let mut best = (start, octile(start, goal));
        let mut reached = false;
        let n = N as i32;
        while let Some(Node(_, cur)) = s.heap.pop() {
            let cur = cur as usize;
            if s.closed[cur] == stamp_id {
                continue;
            }
            s.closed[cur] = stamp_id;
            *budget -= 1;
            if cur == goal {
                reached = true;
                best = (cur, 0.0);
                break;
            }
            let h = octile(cur, goal);
            if h < best.1 {
                best = (cur, h);
            }
            if *budget <= 0 {
                break;
            }
            let (x, z) = ((cur % N) as i32, (cur / N) as i32);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (nx, nz) = (x + dx, z + dz);
                if nx < 0 || nz < 0 || nx >= n || nz >= n {
                    continue;
                }
                let ni = (nz * n + nx) as usize;
                // das Zielfeld selbst darf blockiert sein (Gebäude als Ziel), alles andere nicht
                if field[ni] < need && ni != goal {
                    continue;
                }
                // keine Ecken schneiden
                if dx != 0 && dz != 0 {
                    let a = (z * n + nx) as usize;
                    let b = (nz * n + x) as usize;
                    if field[a] < need || field[b] < need {
                        continue;
                    }
                }
                let mut step = if dx != 0 && dz != 0 { std::f32::consts::SQRT_2 } else { 1.0 } * CELL;
                if breach && let Some((_, wt)) = self.wall_at[ni] && wt != team {
                    step += WALL_COST;
                }
                let ng = s.g[cur] + step;
                if s.seen[ni] != stamp_id || ng < s.g[ni] {
                    s.seen[ni] = stamp_id;
                    s.g[ni] = ng;
                    s.parent[ni] = cur as u32;
                    s.heap.push(Node(ng + octile(ni, goal), ni as u32));
                }
            }
        }
        // Zellen vom Ende zurück zum Start
        let mut cells = vec![best.0];
        let mut c = best.0;
        while c != start {
            c = s.parent[c] as usize;
            cells.push(c);
        }
        cells.reverse();
        // Durchbruch: erstes feindliches Mauerstück auf dem Weg
        let wall = if breach {
            cells.iter().find_map(|c| self.wall_at[*c].filter(|(_, wt)| *wt != team).map(|(e, _)| e))
        } else {
            None
        };
        // Ecken glätten: von jedem Ankerpunkt so weit wie möglich geradeaus
        let mut corners = Vec::new();
        let mut anchor = from;
        let mut i = 0;
        while i + 1 < cells.len() {
            let mut j = i + 1;
            while j + 1 < cells.len() && self.line_clear(team, radius, anchor, center(cells[j + 1]), breach) {
                j += 1;
            }
            anchor = center(cells[j]);
            corners.push(anchor);
            i = j;
        }
        match corners.last_mut() {
            Some(last) if reached => *last = to,
            None => corners.push(if reached { to } else { center(best.0) }),
            _ => {}
        }
        Some(Route { corners, reached, wall })
    }
}

/// Weg einer Einheit
#[derive(Component)]
pub struct Path {
    /// Wofür der Weg berechnet wurde
    pub goal: Vec3,
    pub corners: Vec<Vec3>,
    pub reached: bool,
    pub version: u32,
    /// Fortschrittskontrolle: bester Abstand zur nächsten Ecke und Zeit seitdem
    pub best: f32,
    pub stuck: f32,
}

/// Wegsuchen, die noch ausstehen
#[derive(Resource, Default)]
pub struct PathQueue {
    queue: VecDeque<(Entity, Vec3)>,
    queued: HashSet<Entity>,
}

impl PathQueue {
    pub fn request(&mut self, e: Entity, goal: Vec3) {
        if self.queued.insert(e) {
            self.queue.push_back((e, goal));
        } else if let Some(entry) = self.queue.iter_mut().find(|q| q.0 == e) {
            entry.1 = goal;
        }
    }
}

/// Hindernisse sammeln und das Raster neu aufbauen, wenn Gebäude dazukommen oder verschwinden
pub fn update_grid(
    mut grid: ResMut<NavGrid>,
    added: Query<(), Added<Building>>,
    mut removed: RemovedComponents<Building>,
    q: Query<(Entity, &Transform, &Building, &Team, Has<Wall>, Has<Gate>)>,
) {
    let changed = !added.is_empty() | (removed.read().count() > 0);
    if !changed {
        return;
    }
    let obstacles: Vec<Obstacle> = q
        .iter()
        .map(|(e, t, b, team, wall, gate)| Obstacle {
            pos: t.translation,
            radius: b.radius,
            team: *team,
            entity: e,
            kind: if gate {
                ObstacleKind::Gate
            } else if wall {
                ObstacleKind::Wall
            } else {
                ObstacleKind::Building
            },
        })
        .collect();
    grid.rebuild(&obstacles);
}

/// Ausstehende Wege berechnen, bis das Knotenbudget dieses Frames verbraucht ist
pub fn plan_paths(
    mut commands: Commands,
    grid: Res<NavGrid>,
    mut queue: ResMut<PathQueue>,
    mut search: Local<Option<Box<Search>>>,
    units: Query<(&Transform, &Unit, &Team, Has<Weapon>, Has<AttackMove>, Option<&AttackTarget>)>,
    walls: Query<(), With<Wall>>,
) {
    let s = search.get_or_insert_with(Default::default);
    let mut budget = NODE_BUDGET;
    while budget > 0 {
        let Some((e, goal)) = queue.queue.pop_front() else { break };
        queue.queued.remove(&e);
        let Ok((t, u, team, armed, amove, atk)) = units.get(e) else { continue };
        let Some(route) = grid.find(s, *team, u.radius, t.translation, goal, false, &mut budget) else {
            continue;
        };
        // Ziel hinter Mauern: bewaffnete Einheiten im Angriff brechen durch
        let attacking = amove || atk.is_some_and(|a| !walls.contains(a.0));
        if !route.reached
            && armed
            && attacking
            && let Some(b) = grid.find(s, *team, u.radius, t.translation, goal, true, &mut budget)
            && let Some(wall) = b.wall
        {
            commands.entity(e).insert(AttackTarget(wall));
        }
        commands.entity(e).insert(Path {
            goal,
            corners: route.corners,
            reached: route.reached,
            version: grid.version,
            best: f32::MAX,
            stuck: 0.0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall_line(from: Vec3, to: Vec3, team: Team) -> Vec<Obstacle> {
        let n = (from.distance(to) / 2.0).ceil() as usize;
        (0..=n)
            .map(|k| Obstacle {
                pos: from.lerp(to, k as f32 / n as f32),
                radius: 1.2,
                team,
                entity: Entity::from_raw_u32(k as u32 + 1).expect("Entity"),
                kind: ObstacleKind::Wall,
            })
            .collect()
    }

    #[test]
    fn path_goes_around_a_wall() {
        let mut g = NavGrid::default();
        // Mauer quer über die Strecke, Durchfahrt nur an den Enden
        g.rebuild(&wall_line(Vec3::new(0.0, 0.0, -12.0), Vec3::new(0.0, 0.0, 12.0), ENEMY));
        let mut s = Search::default();
        let mut budget = 100_000;
        let from = Vec3::new(-10.0, 0.0, 0.0);
        let to = Vec3::new(10.0, 0.0, 0.0);
        let r = g.find(&mut s, PLAYER, 1.4, from, to, false, &mut budget).expect("Weg");
        assert!(r.reached);
        assert!(r.corners.len() >= 2, "Umweg erwartet: {:?}", r.corners);
        // der Weg führt um ein Mauerende herum
        assert!(r.corners.iter().any(|c| c.z.abs() > 12.0), "{:?}", r.corners);
        let mut a = from;
        for c in &r.corners {
            assert!(g.line_clear(PLAYER, 1.4, a, *c, false));
            a = *c;
        }
    }

    #[test]
    fn closed_wall_is_breached_at_a_segment() {
        let mut g = NavGrid::default();
        // Ring um das Ziel: kein Durchkommen
        let mut obs = Vec::new();
        for k in 0..24 {
            let a = k as f32 / 24.0 * std::f32::consts::TAU;
            obs.push(Obstacle {
                pos: Vec3::new(20.0 + a.cos() * 8.0, 0.0, a.sin() * 8.0),
                radius: 1.2,
                team: ENEMY,
                entity: Entity::from_raw_u32(k + 1).expect("Entity"),
                kind: ObstacleKind::Wall,
            });
        }
        g.rebuild(&obs);
        let mut s = Search::default();
        let mut budget = 100_000;
        let to = Vec3::new(20.0, 0.0, 0.0);
        let r = g.find(&mut s, PLAYER, 1.4, Vec3::new(-5.0, 0.0, 0.0), to, false, &mut budget).expect("Weg");
        assert!(!r.reached);
        let b = g.find(&mut s, PLAYER, 1.4, Vec3::new(-5.0, 0.0, 0.0), to, true, &mut budget).expect("Durchbruch");
        assert!(b.wall.is_some());
        // Tor statt drei Mauerstücken: der Besitzer fährt durch, der Gegner nicht
        obs.remove(13);
        obs.remove(11);
        obs[11].kind = ObstacleKind::Gate;
        obs[11].radius = 2.6;
        g.rebuild(&obs);
        let from = Vec3::new(-5.0, 0.0, 0.0);
        assert!(g.find(&mut s, ENEMY, 1.9, from, to, false, &mut budget).expect("Weg").reached);
        assert!(!g.find(&mut s, PLAYER, 1.4, from, to, false, &mut budget).expect("Weg").reached);
    }
}
