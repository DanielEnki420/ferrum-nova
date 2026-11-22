//! Spielerbefehle: Was Maus und Tastatur auslösen, wird nicht mehr direkt ins Spiel geschrieben,
//! sondern als `Cmd` gesammelt. Zu Beginn eines Ticks wendet `apply_cmds` die Befehle beider Teams
//! an. Im Duell gehen sie vorher über das Netz an den Mitspieler; Einheiten werden dabei über ihre
//! `NetId` benannt, Positionen als exaktes Bitmuster übertragen.

use crate::components::*;
use crate::orders::{clear_orders, issue_group_move, HoldPosition, Member, MoveKind, Rally};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    /// Gruppe bewegen (mit Formation); `line`: gezogene Frontlinie; `queue`: anhängen
    Move { units: Vec<NetId>, target: Vec3, line: Option<(Vec3, Vec3)>, kind: MoveKind, queue: bool },
    Attack { units: Vec<NetId>, target: NetId },
    /// Sammler auf ein Rohstofffeld
    Harvest { units: Vec<NetId>, node: NetId },
    Stop { units: Vec<NetId> },
    Hold { units: Vec<NetId> },
    Rally { hq: NetId, pos: Vec3, node: Option<NetId> },
    Produce { kind: UnitKind },
    Place { kind: BuildingKind, pos: Vec3, rot: Option<Quat> },
    /// Aufgeben (Duell)
    Surrender,
    /// Eigenes Gebäude abreißen (halbe Kosten zurück)
    Demolish { building: NetId },
    /// Eigenes Gebäude an eine neue Stelle versetzen (baut sich dort neu auf)
    Relocate { building: NetId, pos: Vec3, rot: Option<Quat> },
}

/// Befehle, die die eigene Eingabe seit dem letzten Tick erzeugt hat
#[derive(Resource, Default, Debug)]
pub struct Outbox(pub Vec<Cmd>);

impl Outbox {
    pub fn push(&mut self, c: Cmd) {
        self.0.push(c);
    }
}

/// Befehle für den laufenden Tick, Team 0 zuerst
#[derive(Resource, Default, Debug)]
pub struct TickCmds(pub Vec<(Team, Cmd)>);

// ------------------------------------------------------------------ Kodierung

/// In der Reihenfolge von `idx()` (Deklaration), nicht wie `UnitKind::ALL` (Menü)
const UNIT_KINDS: [UnitKind; 6] = [
    UnitKind::Sammler,
    UnitKind::Panzer,
    UnitKind::Werfer,
    UnitKind::Spaeher,
    UnitKind::Bollwerk,
    UnitKind::Haubitze,
];
const BUILDING_KINDS: [BuildingKind; 6] = [
    BuildingKind::Geschuetzturm,
    BuildingKind::Raffinerie,
    BuildingKind::Fabrik,
    BuildingKind::Mauer,
    BuildingKind::Tor,
    BuildingKind::Raketenturm,
];

#[derive(Default)]
pub struct Writer(pub Vec<u8>);

impl Writer {
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
    fn vec3(&mut self, v: Vec3) {
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
    }
    fn ids(&mut self, ids: &[NetId]) {
        self.u32(ids.len() as u32);
        for i in ids {
            self.u32(i.0);
        }
    }
    fn rot(&mut self, rot: Option<Quat>) {
        match rot {
            Some(q) => {
                self.u8(1);
                for v in q.to_array() {
                    self.f32(v);
                }
            }
            None => self.u8(0),
        }
    }
    pub fn cmd(&mut self, c: &Cmd) {
        match c {
            Cmd::Move { units, target, line, kind, queue } => {
                self.u8(1);
                self.ids(units);
                self.vec3(*target);
                match line {
                    Some((a, b)) => {
                        self.u8(1);
                        self.vec3(*a);
                        self.vec3(*b);
                    }
                    None => self.u8(0),
                }
                self.u8(match kind {
                    MoveKind::Move => 0,
                    MoveKind::AttackMove => 1,
                    MoveKind::Patrol => 2,
                });
                self.u8(*queue as u8);
            }
            Cmd::Attack { units, target } => {
                self.u8(2);
                self.ids(units);
                self.u32(target.0);
            }
            Cmd::Harvest { units, node } => {
                self.u8(3);
                self.ids(units);
                self.u32(node.0);
            }
            Cmd::Stop { units } => {
                self.u8(4);
                self.ids(units);
            }
            Cmd::Hold { units } => {
                self.u8(5);
                self.ids(units);
            }
            Cmd::Rally { hq, pos, node } => {
                self.u8(6);
                self.u32(hq.0);
                self.vec3(*pos);
                self.u32(node.map_or(u32::MAX, |n| n.0));
            }
            Cmd::Produce { kind } => {
                self.u8(7);
                self.u8(kind.idx() as u8);
            }
            Cmd::Place { kind, pos, rot } => {
                self.u8(8);
                self.u8(kind.idx() as u8);
                self.vec3(*pos);
                self.rot(*rot);
            }
            Cmd::Surrender => self.u8(9),
            Cmd::Demolish { building } => {
                self.u8(10);
                self.u32(building.0);
            }
            Cmd::Relocate { building, pos, rot } => {
                self.u8(11);
                self.u32(building.0);
                self.vec3(*pos);
                self.rot(*rot);
            }
        }
    }
}

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    pub fn done(&self) -> bool {
        self.pos >= self.data.len()
    }
    pub fn u8(&mut self) -> Option<u8> {
        let v = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(v)
    }
    pub fn u32(&mut self) -> Option<u32> {
        let b = self.data.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_le_bytes(b.try_into().ok()?))
    }
    pub fn u64(&mut self) -> Option<u64> {
        let b = self.data.get(self.pos..self.pos + 8)?;
        self.pos += 8;
        Some(u64::from_le_bytes(b.try_into().ok()?))
    }
    pub fn f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }
    fn vec3(&mut self) -> Option<Vec3> {
        Some(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
    fn rot(&mut self) -> Option<Option<Quat>> {
        Some(match self.u8()? {
            1 => Some(Quat::from_array([self.f32()?, self.f32()?, self.f32()?, self.f32()?])),
            _ => None,
        })
    }
    fn ids(&mut self) -> Option<Vec<NetId>> {
        let n = self.u32()? as usize;
        if n > 4096 {
            return None;
        }
        (0..n).map(|_| self.u32().map(NetId)).collect()
    }
    pub fn cmd(&mut self) -> Option<Cmd> {
        Some(match self.u8()? {
            1 => {
                let units = self.ids()?;
                let target = self.vec3()?;
                let line = match self.u8()? {
                    1 => Some((self.vec3()?, self.vec3()?)),
                    _ => None,
                };
                let kind = match self.u8()? {
                    0 => MoveKind::Move,
                    1 => MoveKind::AttackMove,
                    2 => MoveKind::Patrol,
                    _ => return None,
                };
                Cmd::Move { units, target, line, kind, queue: self.u8()? != 0 }
            }
            2 => Cmd::Attack { units: self.ids()?, target: NetId(self.u32()?) },
            3 => Cmd::Harvest { units: self.ids()?, node: NetId(self.u32()?) },
            4 => Cmd::Stop { units: self.ids()? },
            5 => Cmd::Hold { units: self.ids()? },
            6 => {
                let hq = NetId(self.u32()?);
                let pos = self.vec3()?;
                let node = self.u32()?;
                Cmd::Rally { hq, pos, node: (node != u32::MAX).then_some(NetId(node)) }
            }
            7 => Cmd::Produce { kind: *UNIT_KINDS.get(self.u8()? as usize)? },
            8 => {
                let kind = *BUILDING_KINDS.get(self.u8()? as usize)?;
                let pos = self.vec3()?;
                Cmd::Place { kind, pos, rot: self.rot()? }
            }
            9 => Cmd::Surrender,
            10 => Cmd::Demolish { building: NetId(self.u32()?) },
            11 => Cmd::Relocate { building: NetId(self.u32()?), pos: self.vec3()?, rot: self.rot()? },
            _ => return None,
        })
    }
}

#[cfg(test)]
pub fn encode(cmds: &[Cmd]) -> Vec<u8> {
    let mut w = Writer::default();
    for c in cmds {
        w.cmd(c);
    }
    w.0
}

#[cfg(test)]
pub fn decode(data: &[u8]) -> Option<Vec<Cmd>> {
    let mut r = Reader::new(data);
    let mut out = Vec::new();
    while !r.done() {
        out.push(r.cmd()?);
    }
    Some(out)
}

// ------------------------------------------------------------------ Anwenden

/// Befehle des Ticks ausführen. Es zählen nur eigene Einheiten des jeweiligen Teams.
/// Punkt aus einem Befehl: nur endliche Werte, auf die Karte begrenzt. Befehle kommen im Duell
/// vom anderen Rechner; ungültige Zahlen (NaN, unendlich, weit draußen) dürfen das Spiel nicht
/// stören. Beide Rechner wenden dieselbe Regel an, die Simulation bleibt gleich.
fn on_map(p: Vec3) -> Option<Vec3> {
    p.is_finite().then(|| Vec3::new(p.x.clamp(-MAP_HALF, MAP_HALF), p.y.clamp(-50.0, 50.0), p.z.clamp(-MAP_HALF, MAP_HALF)))
}

/// Drehung nur als gültige, normierte Quaternion
fn clean_rot(rot: Option<Quat>) -> Option<Quat> {
    rot.filter(|q| q.is_finite() && q.length_squared() > 1e-6).map(|q| q.normalize())
}

pub fn apply_cmds(
    mut commands: Commands,
    mut cmds: ResMut<TickCmds>,
    ids: Query<(Entity, &NetId)>,
    units: Query<(&Transform, &Team, &Unit, Has<Weapon>, Has<Harvester>)>,
    hqs: Query<&Team, With<Hq>>,
    nodes: Query<&Transform, With<ResourceNode>>,
    targets: Query<(), With<Health>>,
    mut harvesters: Query<&mut Harvester>,
    mut builds: MessageWriter<crate::units::BuildRequest>,
    mut places: MessageWriter<crate::construction::PlaceRequest>,
    mut outcome: ResMut<Outcome>,
    mut rebuilds: MessageWriter<crate::construction::RebuildRequest>,
) {
    if cmds.0.is_empty() {
        return;
    }
    let map: HashMap<NetId, Entity> = ids.iter().map(|(e, id)| (*id, e)).collect();
    for (team, cmd) in std::mem::take(&mut cmds.0) {
        let members = |list: &[NetId]| -> Vec<Member> {
            list.iter()
                .filter_map(|id| map.get(id))
                .filter_map(|e| {
                    let (t, ut, u, armed, harvester) = units.get(*e).ok()?;
                    (*ut == team).then_some(Member { e: *e, pos: t.translation, speed: u.speed, radius: u.radius, armed, harvester })
                })
                .collect()
        };
        match cmd {
            Cmd::Move { units: list, target, line, kind, queue } => {
                let Some(target) = on_map(target) else { continue };
                let line = line.and_then(|(a, b)| Some((on_map(a)?, on_map(b)?)));
                let ms = members(&list);
                issue_group_move(&mut commands, &ms, target, line, kind, queue);
                // Sammler, die woanders hinfahren sollen, hören auf zu sammeln
                if kind == MoveKind::Move && !queue {
                    for m in ms.iter().filter(|m| m.harvester) {
                        if let Ok(mut h) = harvesters.get_mut(m.e) {
                            h.state = HarvestState::Idle;
                            h.node = None;
                        }
                    }
                }
            }
            Cmd::Attack { units: list, target } => {
                let Some(&t) = map.get(&target).filter(|t| targets.contains(**t)) else { continue };
                for m in members(&list).iter().filter(|m| m.armed) {
                    let mut ec = commands.entity(m.e);
                    clear_orders(&mut ec);
                    ec.insert(AttackTarget(t));
                }
            }
            Cmd::Harvest { units: list, node } => {
                let Some(&n) = map.get(&node).filter(|n| nodes.contains(**n)) else { continue };
                for m in members(&list).iter().filter(|m| m.harvester) {
                    if let Ok(mut h) = harvesters.get_mut(m.e) {
                        h.node = Some(n);
                        h.state = HarvestState::ToNode;
                    }
                    clear_orders(&mut commands.entity(m.e));
                }
            }
            Cmd::Stop { units: list } => {
                for m in members(&list) {
                    clear_orders(&mut commands.entity(m.e));
                }
            }
            Cmd::Hold { units: list } => {
                for m in members(&list) {
                    let mut ec = commands.entity(m.e);
                    clear_orders(&mut ec);
                    ec.insert(HoldPosition);
                }
            }
            Cmd::Rally { hq, pos, node } => {
                let Some(pos) = on_map(pos) else { continue };
                let Some(&e) = map.get(&hq).filter(|e| hqs.get(**e).is_ok_and(|t| *t == team)) else { continue };
                let node = node.and_then(|n| map.get(&n).copied()).filter(|n| nodes.contains(*n));
                commands.entity(e).insert(Rally { pos, node });
            }
            Cmd::Produce { kind } => {
                builds.write(crate::units::BuildRequest { team, kind });
            }
            Cmd::Place { kind, pos, rot } => {
                let rot = clean_rot(rot);
                if pos.is_finite() && pos.x.abs() <= MAP_HALF && pos.z.abs() <= MAP_HALF {
                    places.write(crate::construction::PlaceRequest { team, kind, pos, rot });
                }
            }
            Cmd::Surrender => {
                if *outcome == Outcome::Running {
                    *outcome = if team == PLAYER { Outcome::Lost } else { Outcome::Won };
                }
            }
            // Ob das Gebäude dem Team gehört, prüft der Bau (construction::rebuild)
            Cmd::Demolish { building } => {
                if let Some(&e) = map.get(&building) {
                    rebuilds.write(crate::construction::RebuildRequest { team, building: e, to: None });
                }
            }
            Cmd::Relocate { building, pos, rot } => {
                if let (Some(&e), true) = (map.get(&building), pos.is_finite() && pos.x.abs() <= MAP_HALF && pos.z.abs() <= MAP_HALF) {
                    rebuilds.write(crate::construction::RebuildRequest { team, building: e, to: Some((pos, clean_rot(rot))) });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_survive_encoding_bit_exact() {
        let cmds = vec![
            Cmd::Move {
                units: vec![NetId(3), NetId(17), NetId(4_000_000)],
                target: Vec3::new(-12.345_678, 0.1, 33.3),
                line: Some((Vec3::new(1.0, 2.0, 3.0), Vec3::new(-0.0, f32::MIN_POSITIVE, 1e-7))),
                kind: MoveKind::Patrol,
                queue: true,
            },
            Cmd::Move { units: vec![], target: Vec3::ZERO, line: None, kind: MoveKind::AttackMove, queue: false },
            Cmd::Attack { units: vec![NetId(1)], target: NetId(99) },
            Cmd::Harvest { units: vec![NetId(5)], node: NetId(2) },
            Cmd::Stop { units: vec![NetId(8), NetId(9)] },
            Cmd::Hold { units: vec![NetId(10)] },
            Cmd::Rally { hq: NetId(0), pos: Vec3::new(5.5, 0.0, -5.5), node: None },
            Cmd::Rally { hq: NetId(0), pos: Vec3::ONE, node: Some(NetId(7)) },
            Cmd::Produce { kind: UnitKind::Haubitze },
            Cmd::Produce { kind: UnitKind::Spaeher },
            Cmd::Produce { kind: UnitKind::Panzer },
            Cmd::Place { kind: BuildingKind::Tor, pos: Vec3::new(-3.0, 0.0, 4.0), rot: Some(Quat::from_rotation_y(0.7)) },
            Cmd::Place { kind: BuildingKind::Raketenturm, pos: Vec3::new(10.0, 0.0, 4.0), rot: None },
            Cmd::Demolish { building: NetId(42) },
            Cmd::Relocate { building: NetId(7), pos: Vec3::new(-1.5, 0.25, 9.0), rot: None },
            Cmd::Relocate { building: NetId(8), pos: Vec3::ONE, rot: Some(Quat::from_rotation_y(-1.2)) },
            Cmd::Surrender,
        ];
        let bytes = encode(&cmds);
        let back = decode(&bytes).expect("lesbar");
        assert_eq!(back, cmds);
        // Bitgenau, auch für -0.0
        if let Cmd::Move { line: Some((_, b)), .. } = &back[0] {
            assert_eq!(b.x.to_bits(), (-0.0f32).to_bits());
        }
        // Abgeschnittene oder kaputte Daten werden abgelehnt, nicht falsch gelesen
        // (das letzte Byte ist „Aufgeben“; zwei Bytes weniger schneiden in den Bauauftrag davor)
        assert!(decode(&bytes[..bytes.len() - 2]).is_none());
        assert!(decode(&[42]).is_none());
        assert_eq!(decode(&[]), Some(vec![]));
        for (i, k) in UNIT_KINDS.iter().enumerate() {
            assert_eq!(k.idx(), i);
        }
        for (i, k) in BUILDING_KINDS.iter().enumerate() {
            assert_eq!(k.idx(), i);
        }
    }

    #[test]
    fn foreign_points_are_kept_on_the_map() {
        assert_eq!(on_map(Vec3::new(1.0, 2.0, 3.0)), Some(Vec3::new(1.0, 2.0, 3.0)));
        assert_eq!(on_map(Vec3::new(1e9, -1e9, -1e9)), Some(Vec3::new(MAP_HALF, -50.0, -MAP_HALF)));
        assert_eq!(on_map(Vec3::new(f32::NAN, 0.0, 0.0)), None);
        assert_eq!(on_map(Vec3::new(0.0, f32::INFINITY, 0.0)), None);
    }
}
