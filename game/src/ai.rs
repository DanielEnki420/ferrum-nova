use crate::components::*;
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
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            think: 0.0,
            next_attack: 150.0,
            wave_size: 5,
            alternate: false,
            elapsed: 0.0,
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

    if queue_len < 2 {
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
