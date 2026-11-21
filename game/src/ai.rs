use crate::components::*;
use crate::construction::{find_spot, PlaceRequest, Site};
use crate::units::BuildRequest;
use bevy::prelude::*;

/// Einfache Gegner-KI: Wirtschaft aufbauen, Armee sammeln, in Wellen angreifen
#[derive(Resource)]
pub struct AiState {
    pub think: f32,
    pub next_attack: f32,
    pub wave_size: usize,
    pub alternate: bool,
    pub elapsed: f32,
    /// Wartezeit bis zum nächsten Gebäude
    pub build_cd: f32,
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            think: 0.0,
            next_attack: 150.0,
            wave_size: 5,
            alternate: false,
            elapsed: 0.0,
            build_cd: 30.0,
        }
    }
}

pub fn enemy_ai(
    mut commands: Commands,
    time: Res<Time>,
    mut ai: ResMut<AiState>,
    mut stock: ResMut<Stock>,
    outcome: Res<Outcome>,
    mut builds: MessageWriter<BuildRequest>,
    units: Query<(Entity, &Team, &Unit, Has<MoveTo>, Has<AttackTarget>)>,
    hqs: Query<(&Team, &Hq)>,
    mut places: MessageWriter<PlaceRequest>,
    structures: Query<(&Team, &Structure)>,
    sites: Query<(&Transform, &Building, &Team)>,
    nodes: Query<&Transform, With<ResourceNode>>,
) {
    if *outcome != Outcome::Running {
        return;
    }
    let dt = time.delta_secs();
    ai.elapsed += dt;
    ai.think -= dt;
    if ai.think > 0.0 {
        return;
    }
    ai.think = 1.0;

    // Kleiner Bonus, damit der Gegner mithalten kann
    stock.0[ENEMY.0 as usize][0] += 1;
    stock.0[ENEMY.0 as usize][1] += 1;

    let mine: Vec<_> = units.iter().filter(|(_, t, _, _, _)| **t == ENEMY).collect();
    let harvesters = mine.iter().filter(|(_, _, u, _, _)| u.kind == UnitKind::Sammler).count();
    let army: Vec<Entity> = mine
        .iter()
        .filter(|(_, _, u, moving, fighting)| u.kind != UnitKind::Sammler && !moving && !fighting)
        .map(|(e, ..)| *e)
        .collect();

    let queue_len = hqs
        .iter()
        .find(|(t, _)| **t == ENEMY)
        .map(|(_, hq)| hq.queue.len())
        .unwrap_or(99);

    // Gebäude: erst eine Raffinerie am nächsten Feld, dann Geschütztürme zur Front, später eine Fabrik
    ai.build_cd -= 1.0;
    let count = |k: BuildingKind| structures.iter().filter(|(t, s)| **t == ENEMY && s.0 == k).count();
    let turrets_wanted = if ai.elapsed < 80.0 { 0 } else { (1 + ((ai.elapsed - 80.0) / 70.0) as usize).min(4) };
    let goal = if count(BuildingKind::Raffinerie) == 0 && ai.elapsed > 35.0 {
        Some(BuildingKind::Raffinerie)
    } else if count(BuildingKind::Geschuetzturm) < turrets_wanted {
        Some(BuildingKind::Geschuetzturm)
    } else if count(BuildingKind::Fabrik) == 0 && ai.elapsed > 200.0 {
        Some(BuildingKind::Fabrik)
    } else {
        None
    };
    let mut saving = false;
    if let Some(kind) = goal
        && ai.build_cd <= 0.0
    {
        if stock.can_afford(ENEMY, kind.cost()) {
            let site_list: Vec<Site> = sites.iter().map(|(t, b, team)| Site { pos: t.translation, radius: b.radius, team: *team }).collect();
            let node_list: Vec<Vec3> = nodes.iter().map(|t| t.translation).collect();
            let to_player = (PLAYER_HQ_POS - ENEMY_HQ_POS).normalize();
            let target = match kind {
                BuildingKind::Raffinerie => {
                    // zwischen HQ und dem nächsten Feld
                    let field = node_list
                        .iter()
                        .min_by(|a, b| a.distance(ENEMY_HQ_POS).total_cmp(&b.distance(ENEMY_HQ_POS)))
                        .copied()
                        .unwrap_or(ENEMY_HQ_POS);
                    ENEMY_HQ_POS.lerp(field, 0.55)
                }
                BuildingKind::Geschuetzturm => {
                    let side = Vec3::new(-to_player.z, 0.0, to_player.x);
                    let n = count(BuildingKind::Geschuetzturm) as f32;
                    ENEMY_HQ_POS + to_player * 13.0 + side * (if (n as usize).is_multiple_of(2) { 1.0 } else { -1.0 }) * (4.0 + n * 3.0)
                }
                BuildingKind::Fabrik => ENEMY_HQ_POS - to_player * 2.0 + Vec3::new(to_player.z, 0.0, -to_player.x) * 11.0,
            };
            if let Some(pos) = find_spot(kind, target, ENEMY, &site_list, &node_list) {
                places.write(PlaceRequest { team: ENEMY, kind, pos });
                ai.build_cd = 12.0;
            } else {
                ai.build_cd = 20.0;
            }
        } else {
            saving = true;
        }
    }

    if queue_len < 2 && (!saving || harvesters < 4) {
        let kind = if harvesters < 4 {
            UnitKind::Sammler
        } else {
            ai.alternate = !ai.alternate;
            if ai.alternate { UnitKind::Panzer } else { UnitKind::Werfer }
        };
        builds.write(BuildRequest { team: ENEMY, kind });
    }

    if ai.elapsed >= ai.next_attack && army.len() >= ai.wave_size {
        for e in &army {
            commands.entity(*e).insert((MoveTo(PLAYER_HQ_POS), AttackMove));
        }
        ai.wave_size = (ai.wave_size + 2).min(16);
        ai.next_attack = ai.elapsed + 75.0;
    }
}
