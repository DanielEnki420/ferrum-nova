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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnitKind {
    Sammler,
    Panzer,
    Werfer,
}

impl UnitKind {
    pub fn name(self) -> &'static str {
        match self {
            UnitKind::Sammler => "Sammler",
            UnitKind::Panzer => "Panzer",
            UnitKind::Werfer => "Raketenwerfer",
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
        }
    }
    pub fn build_time(self) -> f32 {
        match self {
            UnitKind::Sammler => 4.0,
            UnitKind::Panzer => 5.0,
            UnitKind::Werfer => 6.5,
        }
    }
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
}

/// Bewegungsziel auf dem Boden
#[derive(Component)]
pub struct MoveTo(pub Vec3);

/// Expliziter Angriffsbefehl
#[derive(Component)]
pub struct AttackTarget(pub Entity);

/// Angriffsbewegung: unterwegs auf alles schießen, was in Sicht kommt
#[derive(Component)]
pub struct AttackMove;

#[derive(Component)]
pub struct Selected;

#[derive(Component)]
pub struct Building {
    pub radius: f32,
}

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

#[derive(Resource, Default, PartialEq, Eq, Clone, Copy)]
pub enum Outcome {
    #[default]
    Running,
    Won,
    Lost,
}

/// Wiederverwendbare Meshes und Materialien
#[derive(Resource)]
pub struct Assets3d {
    pub sammler: Handle<Mesh>,
    pub panzer: Handle<Mesh>,
    pub werfer: Handle<Mesh>,
    pub turret: Handle<Mesh>,
    pub projectile: Handle<Mesh>,
    pub team_mat: [Handle<StandardMaterial>; 2],
    pub accent_mat: [Handle<StandardMaterial>; 2],
    pub projectile_mat: [Handle<StandardMaterial>; 2],
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
