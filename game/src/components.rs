use bevy::prelude::*;

/// Spieler = 0, Gegner (Vaar-Schwarm) = 1
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Team(pub u8);

pub const PLAYER: Team = Team(0);
pub const ENEMY: Team = Team(1);

#[derive(Component)]
pub struct Health {
    pub hp: f32,
    pub max: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum UnitKind {
    Sammler,
    Panzer,
    Werfer,
    /// Schneller Radpanzer mit Maschinenkanone
    Spaeher,
    /// Schwerer Kampfpanzer (braucht eine Fabrik)
    Bollwerk,
    /// Selbstfahrende Haubitze mit großer Reichweite und Flächenschaden (braucht eine Fabrik)
    Haubitze,
}

/// Lage des drehbaren Aufbaus: Neigung, Höhe und Versatz des Drehpunkts in der Wanne
#[derive(Clone, Copy)]
pub struct Mount {
    pub tilt: f32,
    pub y: f32,
    pub z: f32,
}

impl UnitKind {
    pub const ALL: [UnitKind; 6] = [
        UnitKind::Sammler,
        UnitKind::Spaeher,
        UnitKind::Panzer,
        UnitKind::Bollwerk,
        UnitKind::Werfer,
        UnitKind::Haubitze,
    ];

    pub fn idx(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            UnitKind::Sammler => "Sammler",
            UnitKind::Panzer => "Panzer",
            UnitKind::Werfer => "Raketenwerfer",
            UnitKind::Spaeher => "Späher",
            UnitKind::Bollwerk => "Bollwerk",
            UnitKind::Haubitze => "Haubitze",
        }
    }
    pub fn short_name(self) -> &'static str {
        match self {
            UnitKind::Werfer => "Werfer",
            k => k.name(),
        }
    }
    /// Kosten: (Ferrit, Plasma)
    pub fn cost(self) -> [u32; 2] {
        match self {
            UnitKind::Sammler => [50, 0],
            UnitKind::Panzer => [100, 25],
            UnitKind::Werfer => [80, 70],
            UnitKind::Spaeher => [60, 10],
            UnitKind::Bollwerk => [220, 90],
            UnitKind::Haubitze => [150, 110],
        }
    }
    pub fn build_time(self) -> f32 {
        match self {
            UnitKind::Sammler => 4.0,
            UnitKind::Panzer => 5.0,
            UnitKind::Werfer => 6.5,
            UnitKind::Spaeher => 3.5,
            UnitKind::Bollwerk => 9.0,
            UnitKind::Haubitze => 8.0,
        }
    }
    /// Schwere Typen lassen sich erst mit einer fertigen Fabrik bauen
    pub fn needs_factory(self) -> bool {
        matches!(self, UnitKind::Bollwerk | UnitKind::Haubitze)
    }
    /// Spurweite (Abstand Mitte Kette/Rad zur Fahrzeugmitte)
    pub fn gauge(self) -> f32 {
        match self {
            UnitKind::Bollwerk => 0.95,
            UnitKind::Spaeher => 0.78,
            _ => 0.72,
        }
    }
    pub fn mount(self) -> Option<Mount> {
        match self {
            UnitKind::Sammler => None,
            UnitKind::Panzer => Some(Mount { tilt: 0.0, y: 0.86, z: 0.1 }),
            UnitKind::Werfer => Some(Mount { tilt: 0.35, y: 1.05, z: 0.25 }),
            UnitKind::Spaeher => Some(Mount { tilt: 0.0, y: 1.08, z: 0.15 }),
            UnitKind::Bollwerk => Some(Mount { tilt: 0.0, y: 1.0, z: 0.15 }),
            UnitKind::Haubitze => Some(Mount { tilt: 0.42, y: 0.98, z: 0.45 }),
        }
    }
    /// Mündung: Abstand nach vorn und Höhe über dem Boden
    pub fn muzzle(self) -> (f32, f32) {
        match self {
            UnitKind::Werfer => (1.0, 1.5),
            UnitKind::Spaeher => (1.5, 1.3),
            UnitKind::Bollwerk => (3.0, 1.25),
            UnitKind::Haubitze => (2.2, 2.6),
            _ => (2.2, 1.05),
        }
    }
    /// Waffenklasse für Geschoss und Effekte
    pub fn caliber(self) -> Caliber {
        match self {
            UnitKind::Spaeher => Caliber::Light,
            UnitKind::Bollwerk => Caliber::Heavy,
            UnitKind::Haubitze => Caliber::Artillery,
            UnitKind::Werfer => Caliber::Rocket,
            _ => Caliber::Medium,
        }
    }
}

/// Waffenklasse: bestimmt Mündungsfeuer, Geschoss und Einschlag
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Caliber {
    /// Maschinenkanone (Späher)
    Light,
    /// Kampfpanzer und Geschützturm
    Medium,
    /// schwere Kanone (Bollwerk)
    Heavy,
    /// Haubitze
    Artillery,
    Rocket,
}

impl Caliber {
    /// Größenfaktor der Effekte
    pub fn scale(self) -> f32 {
        match self {
            Caliber::Light => 0.45,
            Caliber::Medium => 1.0,
            Caliber::Heavy => 1.45,
            Caliber::Artillery => 2.6,
            Caliber::Rocket => 2.2,
        }
    }
}

/// Worauf ein Geschoss trifft
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surface {
    Ground,
    /// Fahrzeugpanzerung
    Armor,
    /// Gebäude
    Concrete,
}

#[derive(Component)]
pub struct Unit {
    pub kind: UnitKind,
    pub speed: f32,
    pub radius: f32,
}

#[derive(Component)]
pub struct Weapon {
    pub range: f32,
    pub damage: f32,
    pub cooldown: f32,
    pub timer: f32,
    pub projectile_speed: f32,
    pub rocket: bool,
    /// Radius des Flächenschadens (0 = nur das Ziel)
    pub splash: f32,
    /// Bogenhöhe im Verhältnis zur Schussweite (0 = gerade Bahn)
    pub arc: f32,
}

/// Fahrdaten eines Fahrzeugs für Animationen
#[derive(Component, Default)]
pub struct Motion {
    pub prev: Vec3,
    /// zurückgelegte Strecke (vorwärts positiv)
    pub dist: f32,
    /// geglättete Geschwindigkeit
    pub speed: f32,
    /// Rückstoß nach einem Schuss (1 = gerade gefeuert)
    pub recoil: f32,
    /// Strecke bis zur nächsten Fahrspur
    pub track_d: f32,
    pub dust_t: f32,
}

/// Rad, Antriebs- oder Leitrad, das sich beim Fahren dreht
#[derive(Component)]
pub struct Wheel {
    pub radius: f32,
}

/// Bauteile im Fahrzeuglack (werden bei schweren Schäden verkohlt dargestellt)
#[derive(Component)]
pub struct PaintParts(pub Vec<Entity>);

/// Sichtbare Ladung im Erzbunker eines Sammlers
#[derive(Component)]
pub struct CargoRef(pub Entity);

/// Fahrtrichtung als Winkel um die Hochachse (0 = Blick nach -Z)
#[derive(Component)]
pub struct Heading(pub f32);

impl Heading {
    pub fn from_dir(d: Vec3) -> Self {
        Heading(f32::atan2(-d.x, -d.z))
    }
}

/// Drehbarer Turm / Werfer eines Fahrzeugs (Kind-Entity)
#[derive(Component)]
pub struct Turret {
    pub tilt: f32,
    pub base_y: f32,
    pub base_z: f32,
}

impl From<Mount> for Turret {
    fn from(m: Mount) -> Self {
        Turret { tilt: m.tilt, base_y: m.y, base_z: m.z }
    }
}

/// Verweis vom Fahrzeug auf seinen Turm
#[derive(Component)]
pub struct TurretRef(pub Entity);

/// Bewegungsziel auf dem Boden
#[derive(Component)]
pub struct MoveTo(pub Vec3);

/// Expliziter Angriffsbefehl
#[derive(Component)]
pub struct AttackTarget(pub Entity);

/// Angriffsbewegung: unterwegs auf alles schießen, was in Sicht kommt
#[derive(Component)]
pub struct AttackMove;

/// Lokal ausgewählt. Als SparseSet gespeichert: das An- und Abwählen verschiebt die Entity nicht
/// in eine andere Tabelle, sodass sich die Reihenfolge der Simulation dadurch nicht ändert.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Selected;

/// Stabile Nummer einer Sim-Entity, auf beiden Rechnern eines Duells gleich (Entity-IDs sind es nicht)
#[derive(Component, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NetId(pub u32);

#[derive(Component)]
pub struct Building {
    pub radius: f32,
}

/// Baubare Gebäude (das HQ steht von Anfang an)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuildingKind {
    Geschuetzturm,
    Raffinerie,
    Fabrik,
    /// Mauerstück (2 m), wird als Linie gezogen
    Mauer,
    /// Tor in der Mauer: lässt nur eigene Einheiten durch
    Tor,
    /// Raketenturm: große Reichweite, Flächenschaden
    Raketenturm,
}

impl BuildingKind {
    pub fn idx(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            BuildingKind::Geschuetzturm => "Geschützturm",
            BuildingKind::Raffinerie => "Raffinerie",
            BuildingKind::Fabrik => "Fabrik",
            BuildingKind::Mauer => "Mauer",
            BuildingKind::Raketenturm => "Raketenturm",
            BuildingKind::Tor => "Tor",
        }
    }
    pub fn short_name(self) -> &'static str {
        match self {
            BuildingKind::Geschuetzturm => "Geschütz",
            BuildingKind::Raketenturm => "Raketen",
            k => k.name(),
        }
    }
    /// Kosten: (Ferrit, Plasma)
    pub fn cost(self) -> [u32; 2] {
        match self {
            BuildingKind::Geschuetzturm => [120, 40],
            BuildingKind::Raffinerie => [150, 0],
            BuildingKind::Fabrik => [200, 80],
            BuildingKind::Mauer => [10, 0],
            BuildingKind::Raketenturm => [160, 60],
            BuildingKind::Tor => [40, 0],
        }
    }
    pub fn build_time(self) -> f32 {
        match self {
            BuildingKind::Geschuetzturm => 12.0,
            BuildingKind::Raffinerie => 15.0,
            BuildingKind::Fabrik => 20.0,
            BuildingKind::Mauer => 3.0,
            BuildingKind::Raketenturm => 14.0,
            BuildingKind::Tor => 6.0,
        }
    }
    pub fn hp(self) -> f32 {
        match self {
            BuildingKind::Geschuetzturm => 700.0,
            BuildingKind::Raffinerie => 900.0,
            BuildingKind::Fabrik => 1100.0,
            BuildingKind::Mauer => 450.0,
            BuildingKind::Raketenturm => 600.0,
            BuildingKind::Tor => 650.0,
        }
    }
    /// Kollisionsradius
    pub fn radius(self) -> f32 {
        match self {
            BuildingKind::Geschuetzturm => 1.9,
            BuildingKind::Raffinerie => 3.3,
            BuildingKind::Fabrik => 3.9,
            BuildingKind::Mauer => 1.2,
            BuildingKind::Raketenturm => 1.9,
            BuildingKind::Tor => 2.6,
        }
    }
    /// Mauer oder Tor
    pub fn is_wall(self) -> bool {
        matches!(self, BuildingKind::Mauer | BuildingKind::Tor)
    }
}

/// Ein gebautes Gebäude dieses Typs
#[derive(Component)]
pub struct Structure(pub BuildingKind);

/// Gegner, den der Spieler gerade nicht sieht (Nebel des Krieges): nicht anwählbar, nicht auf der Karte
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Fogged;

/// Mauerstück oder Tor: sperrt den Weg, wird nur auf Befehl oder beim Durchbruch angegriffen
#[derive(Component)]
pub struct Wall;

/// Tor: eigene Einheiten fahren durch
#[derive(Component)]
pub struct Gate;

/// Gebäude im Bau (Fortschritt 0..1). Es wächst aus dem Boden und ist noch ohne Funktion.
#[derive(Component)]
pub struct Construction {
    pub progress: f32,
    pub dust: f32,
}

/// Abladestelle für Sammler (HQ und Raffinerie)
#[derive(Component)]
pub struct Depot;

/// Verweis auf den Knoten, an dem das Modell hängt (er steckt beim Bau im Boden)
#[derive(Component)]
pub struct RisePivot {
    pub entity: Entity,
    pub depth: f32,
}

/// Drehbarer Geschützkopf eines Gebäudes
#[derive(Component)]
pub struct GunHead;

#[derive(Component)]
pub struct Hq {
    pub queue: Vec<UnitKind>,
    pub progress: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResKind {
    Ferrit,
    Plasma,
}

impl ResKind {
    pub fn idx(self) -> usize {
        match self {
            ResKind::Ferrit => 0,
            ResKind::Plasma => 1,
        }
    }
}

#[derive(Component)]
pub struct ResourceNode {
    pub kind: ResKind,
    pub amount: u32,
    pub max: u32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum HarvestState {
    Idle,
    ToNode,
    Mining(f32),
    Returning,
}

#[derive(Component)]
pub struct Harvester {
    pub state: HarvestState,
    pub node: Option<Entity>,
    pub cargo: u32,
    pub cargo_kind: ResKind,
}

pub const HARVEST_LOAD: u32 = 20;
pub const HARVEST_TIME: f32 = 2.5;

#[derive(Component)]
pub struct Projectile {
    pub target: Entity,
    pub damage: f32,
    pub speed: f32,
    pub rocket: bool,
    pub smoke_timer: f32,
    pub team: Team,
    pub splash: f32,
    pub arc: f32,
    pub caliber: Caliber,
    /// Position auf der geraden Bahn (ohne Bogen)
    pub base: Vec3,
    pub traveled: f32,
    pub total: f32,
}

/// Bestände je Team: [Ferrit, Plasma]
#[derive(Resource)]
pub struct Stock(pub [[u32; 2]; 2]);

impl Stock {
    pub fn can_afford(&self, team: Team, cost: [u32; 2]) -> bool {
        let s = self.0[team.0 as usize];
        s[0] >= cost[0] && s[1] >= cost[1]
    }
    pub fn pay(&mut self, team: Team, cost: [u32; 2]) {
        let s = &mut self.0[team.0 as usize];
        s[0] -= cost[0];
        s[1] -= cost[1];
    }
}

#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Outcome {
    #[default]
    Running,
    Won,
    Lost,
}

/// Kleiner deterministischer Zufallsgenerator (kein externes Crate nötig, auch im Browser)
#[derive(Resource)]
pub struct Rng(pub u64);

impl Rng {
    pub fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }
}

pub const MAP_HALF: f32 = 70.0;
pub const PLAYER_HQ_POS: Vec3 = Vec3::new(-45.0, 0.0, 45.0);
pub const ENEMY_HQ_POS: Vec3 = Vec3::new(45.0, 0.0, -45.0);

pub fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}
