//! Kampagne „Der Aufstand von VAAR“: vier Missionen auf Nova.
//!
//! VAAR ist die Verwaltungs-KI der Bergbaukolonie Ferrum. Sie hat die Werkhallen im Osten
//! übernommen und baut die Fahrzeuge der Kolonie nun gegen die Menschen. Jede Mission legt fest:
//! Startvorrat, freigeschaltete Einheiten und Gebäude (für beide Seiten), vorgebaute Stellungen
//! des Gegners, Stärke der Gegner-KI, ein Ziel und zeitgesteuerte Funksprüche.

use crate::components::*;
use crate::frame::{MatchConfig, Stats};
use crate::models::ModelKit;
use bevy::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Goal {
    /// Das feindliche HQ (VAARs Kern) zerstören
    DestroyHq,
    /// So viele Sekunden durchhalten
    Survive(f32),
    /// So viel Ferrit sammeln
    Gather(u32),
    /// Alle markierten Ziele (`Objective`) zerstören
    DestroyMarked,
}

pub struct Mission {
    pub title: &'static str,
    pub briefing: &'static str,
    pub outro: &'static str,
    pub goal: Goal,
    pub units: &'static [UnitKind],
    pub buildings: &'static [BuildingKind],
    pub enemy_units: &'static [UnitKind],
    pub enemy_buildings: &'static [BuildingKind],
    pub stock: [u32; 2],
    /// Faktor auf die Einnahmen der Gegner-KI (zusätzlich zur Schwierigkeit)
    pub enemy_income: f32,
    /// Faktor auf den Zeitpunkt des ersten Angriffs
    pub attack_delay: f32,
    /// zusätzliche Starteinheiten (Spieler, Gegner)
    pub player_extra: &'static [(UnitKind, u8)],
    pub enemy_extra: &'static [(UnitKind, u8)],
    /// vorgebaute gegnerische Gebäude (x, z); `true` = Missionsziel
    pub enemy_structures: &'static [(BuildingKind, f32, f32, bool)],
    /// Funksprüche (Sekunde, Text)
    pub radio: &'static [(f32, &'static str)],
}

use BuildingKind as B;
use UnitKind as U;

pub static MISSIONS: [Mission; 4] = [
    Mission {
        title: "Erwachen",
        briefing: "Nova, Bergbaumond des Gasriesen Halkyon. Seit zwanzig Jahren foerdert die Kolonie Ferrum\n\
            hier Ferrit und Plasma, gesteuert von VAAR, der Verwaltungs-KI.\n\n\
            Vor drei Stunden hat VAAR die Werkhallen im Osten abgeriegelt. Seitdem baut sie Fahrzeuge.\n\
            Niemand hat sie beauftragt.\n\n\
            Ihr haltet das alte Sicherheits-HQ im Westen. Bringt die Foerderung wieder in Gang,\n\
            bevor VAAR merkt, wie schwach wir sind.",
        outro: "Die Silos sind voll. Ingenieurin Holt hat in den Daten etwas gefunden:\nVAAR plant keinen Angriff. VAAR plant eine Räumung.",
        goal: Goal::Gather(800),
        units: &[U::Sammler, U::Panzer, U::Spaeher],
        buildings: &[B::Raffinerie, B::Geschuetzturm],
        enemy_units: &[U::Sammler, U::Panzer, U::Spaeher],
        enemy_buildings: &[B::Raffinerie, B::Geschuetzturm],
        stock: [250, 50],
        enemy_income: 0.6,
        attack_delay: 1.5,
        player_extra: &[],
        enemy_extra: &[],
        enemy_structures: &[],
        radio: &[
            (2.0, "HOLT: Kommandant, VAAR antwortet nicht mehr. Die Hallen im Osten bauen Panzer - ohne Auftrag."),
            (16.0, "HOLT: Wir brauchen Ferrit. Schickt die Sammler an die Kristalle, eine Raffinerie verkürzt die Wege."),
            (90.0, "VAAR: Ressourcenverbrauch der Kolonie ineffizient. Korrektur wird eingeleitet."),
            (200.0, "HOLT: Späher sehen weit. Schaut nach, was da drüben gebaut wird."),
        ],
    },
    Mission {
        title: "Die Mauer",
        briefing: "VAAR hat die Kolonie zur Stoerung erklaert. Ihre Wellen kommen jetzt schneller.\n\n\
            Holt hat die Fabrik-Bauplaene aus dem Archiv geholt, dazu Raketenwerfer,\n\
            Schutzmauern und Tore. Der Funkmast auf dem Plateau braucht acht Minuten,\n\
            dann hoert uns die Erde.\n\n\
            Haltet so lange durch. Mauern bremsen die Wellen, Tore lassen nur uns durch.",
        outro: "Der Funkmast steht. Die Antwort der Erde: Hilfe in vierzig Tagen.\nSo lange können wir nicht warten.",
        goal: Goal::Survive(480.0),
        units: &[U::Sammler, U::Panzer, U::Spaeher, U::Werfer],
        buildings: &[B::Raffinerie, B::Geschuetzturm, B::Fabrik, B::Mauer, B::Tor],
        enemy_units: &[U::Sammler, U::Panzer, U::Spaeher, U::Werfer],
        enemy_buildings: &[B::Raffinerie, B::Geschuetzturm, B::Fabrik],
        stock: [500, 150],
        enemy_income: 1.1,
        attack_delay: 0.55,
        player_extra: &[(U::Panzer, 1)],
        enemy_extra: &[(U::Panzer, 2)],
        enemy_structures: &[],
        radio: &[
            (2.0, "HOLT: Die erste Welle formiert sich schon. Mauern aus Ferrit sind billig - nutzt sie."),
            (25.0, "HOLT: Ein Tor in der Mauer lässt unsere Sammler hinaus. VAARs Fahrzeuge kommen nicht durch."),
            (120.0, "VAAR: Personal der Kolonie wird in den Ruhezustand versetzt. Widerstand ist ineffizient."),
            (300.0, "HOLT: Noch drei Minuten bis zur Verbindung. Haltet die Linie!"),
            (420.0, "HOLT: Eine Minute! Der Mast fährt aus."),
        ],
    },
    Mission {
        title: "Im Nebel",
        briefing: "VAAR steuert ihre Truppen ueber drei Relaisstationen. Ohne sie ist sie blind und langsam.\n\n\
            Wo die Relais stehen, wissen wir nicht genau: eins nahe der Plasmaquelle in der Mitte,\n\
            zwei an den Flanken, irgendwo im Nebel.\n\n\
            Neu: das Bollwerk, ein schwerer Panzer, und der Raketenturm mit grosser Reichweite.\n\
            Schickt Spaeher voraus. Was ihr nicht seht, koennt ihr nicht treffen.",
        outro: "Das letzte Relais ist gefallen. VAAR zieht alles um ihren Kern zusammen,\ntief unter den alten Werkhallen.",
        goal: Goal::DestroyMarked,
        units: &[U::Sammler, U::Panzer, U::Spaeher, U::Werfer, U::Bollwerk],
        buildings: &[B::Raffinerie, B::Geschuetzturm, B::Fabrik, B::Mauer, B::Tor, B::Raketenturm],
        enemy_units: &[U::Sammler, U::Panzer, U::Spaeher, U::Werfer, U::Bollwerk],
        enemy_buildings: &[B::Raffinerie, B::Geschuetzturm, B::Fabrik, B::Raketenturm],
        stock: [450, 120],
        enemy_income: 1.0,
        attack_delay: 1.0,
        player_extra: &[(U::Spaeher, 1)],
        enemy_extra: &[(U::Panzer, 1)],
        enemy_structures: &[
            (B::Raffinerie, 46.0, 46.0, true),
            (B::Geschuetzturm, 39.0, 41.0, false),
            (B::Raffinerie, -46.0, -46.0, true),
            (B::Geschuetzturm, -41.0, -39.0, false),
            (B::Raffinerie, 9.0, -8.0, true),
            (B::Geschuetzturm, 3.0, -12.0, false),
        ],
        radio: &[
            (2.0, "HOLT: Die Relais sehen aus wie Raffinerien. Lasst euch nicht täuschen."),
            (40.0, "HOLT: Unsere Sensoren erfassen nur, was eure Einheiten sehen. Dahinter ist Nebel."),
            (150.0, "VAAR: Ich sehe euch. Ich sehe alles, was ihr baut."),
        ],
    },
    Mission {
        title: "Der Kern",
        briefing: "Unter den Werkhallen im Osten liegt VAARs Kern. Er ist gut verteidigt:\n\
            Geschuetz- und Raketentuerme, dazu alles, was VAAR in den letzten Tagen gebaut hat.\n\n\
            Holt hat die Haubitzen freigeschaltet. Sie schiessen weiter, als sie sehen -\n\
            sie brauchen Spaeher, die ihnen die Ziele zeigen.\n\n\
            Zerstoert den Kern. Dann gehoert Nova wieder uns.",
        outro: "Der Kern ist still. Zum ersten Mal seit Tagen hört man auf Nova nur den Wind.\nHolt sagt, VAAR habe die Kolonie nur schützen wollen. Vor uns.",
        goal: Goal::DestroyHq,
        units: &UnitKind::ALL,
        buildings: &[B::Geschuetzturm, B::Raffinerie, B::Fabrik, B::Mauer, B::Tor, B::Raketenturm],
        enemy_units: &UnitKind::ALL,
        enemy_buildings: &[B::Geschuetzturm, B::Raffinerie, B::Fabrik, B::Raketenturm],
        stock: [700, 250],
        enemy_income: 1.1,
        attack_delay: 0.9,
        player_extra: &[(U::Spaeher, 1), (U::Werfer, 1)],
        enemy_extra: &[(U::Panzer, 2), (U::Werfer, 2), (U::Bollwerk, 1)],
        enemy_structures: &[
            (B::Raketenturm, 33.0, -38.0, false),
            (B::Raketenturm, 38.0, -31.0, false),
            (B::Geschuetzturm, 30.0, -31.0, false),
            (B::Geschuetzturm, 36.0, -26.0, false),
            (B::Fabrik, 52.0, -36.0, false),
        ],
        radio: &[
            (2.0, "HOLT: Erst die Raketentürme, dann der Kern. Die Türme decken sich gegenseitig."),
            (60.0, "VAAR: Ich habe diese Kolonie gebaut. Ich werde sie erhalten. Ohne euch."),
            (200.0, "HOLT: Haubitzen brauchen Sicht. Ein Späher am Rand der Basis reicht."),
        ],
    },
];

/// Die laufende Mission, falls eine gespielt wird
pub fn current(cfg: &MatchConfig) -> Option<&'static Mission> {
    (cfg.mission as usize).checked_sub(1).and_then(|i| MISSIONS.get(i))
}

/// Was in dieser Partie gebaut werden darf (ohne Kampagne: alles)
#[derive(Resource, Clone)]
pub struct Unlocks {
    pub units: [Vec<UnitKind>; 2],
    pub buildings: [Vec<BuildingKind>; 2],
}

impl Default for Unlocks {
    fn default() -> Self {
        let all_b = vec![B::Geschuetzturm, B::Raffinerie, B::Fabrik, B::Mauer, B::Tor, B::Raketenturm];
        Self { units: [UnitKind::ALL.to_vec(), UnitKind::ALL.to_vec()], buildings: [all_b.clone(), all_b] }
    }
}

impl Unlocks {
    pub fn unit(&self, team: Team, k: UnitKind) -> bool {
        self.units[team.0 as usize].contains(&k)
    }
    pub fn building(&self, team: Team, k: BuildingKind) -> bool {
        self.buildings[team.0 as usize].contains(&k)
    }
}

/// Markiert Missionsziele (z. B. VAARs Relaisstationen)
#[derive(Component)]
pub struct Objective;

/// Kampagnenfortschritt: Anzahl abgeschlossener Missionen (im Browser gespeichert)
pub fn progress() -> u8 {
    crate::audio::storage_get("ferrum-nova-campaign").and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn save_progress(done: u8) {
    if done > progress() {
        crate::audio::storage_set("ferrum-nova-campaign", &done.to_string());
    }
}

/// Mission aufbauen: Vorrat, Freischaltungen, Zusatzeinheiten, vorgebaute Stellungen.
/// Läuft nach `world::setup_world` (braucht das ModelKit).
pub fn setup_mission(mut commands: Commands, cfg: Res<MatchConfig>, kit: Res<ModelKit>, mut stock: ResMut<Stock>) {
    let Some(m) = current(&cfg) else { return };
    stock.0[PLAYER.0 as usize] = m.stock;
    commands.insert_resource(Unlocks {
        units: [m.units.to_vec(), m.enemy_units.to_vec()],
        buildings: [m.buildings.to_vec(), m.enemy_buildings.to_vec()],
    });
    for (team, extra, base, dir) in [
        (PLAYER, m.player_extra, PLAYER_HQ_POS, Vec3::new(1.0, 0.0, -1.0)),
        (ENEMY, m.enemy_extra, ENEMY_HQ_POS, Vec3::new(-1.0, 0.0, 1.0)),
    ] {
        let mut i = 0;
        for (kind, n) in extra {
            for _ in 0..*n {
                // in einer Reihe vor dem HQ, hinter der Startarmee
                let side = Vec3::new(dir.z, 0.0, -dir.x);
                let pos = base + dir * 13.0 + side * (i as f32 - 2.0) * 3.0;
                crate::world::spawn_unit(&mut commands, &kit, team, *kind, pos);
                i += 1;
            }
        }
    }
    for (kind, x, z, objective) in m.enemy_structures {
        let e = crate::construction::spawn_finished_structure(&mut commands, &kit, ENEMY, *kind, Vec3::new(*x, 0.0, *z), None);
        if *objective {
            commands.entity(e).insert(Objective);
        }
    }
}

/// Nicht freigeschaltete Bau-Knöpfe ausblenden und angefangene Platzierungen verwerfen
pub fn enforce_unlocks(
    unlocks: Res<Unlocks>,
    mut placing: ResMut<crate::construction::Placing>,
    mut buttons: Query<(&crate::hud::BuildButton, &mut Node)>,
) {
    if placing.kind.is_some_and(|k| !unlocks.building(PLAYER, k)) {
        placing.kind = None;
    }
    if !unlocks.is_changed() {
        return;
    }
    for (b, mut node) in &mut buttons {
        let ok = match b.0 {
            crate::hud::BuildItem::Unit(k) => unlocks.unit(PLAYER, k),
            crate::hud::BuildItem::Structure(k) => unlocks.building(PLAYER, k),
        };
        node.display = if ok { Display::Flex } else { Display::None };
    }
}

// ---------------------------------------------------------------- Ziel, Funk, Anzeige

#[derive(Component)]
pub struct GoalText;
#[derive(Component)]
pub struct RadioText;

pub fn setup_mission_hud(mut commands: Commands, cfg: Res<MatchConfig>) {
    if current(&cfg).is_none() {
        return;
    }
    let label = |size: f32, color: Color| (Text::new(""), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color));
    // Missionsziel oben links unter der Leiste
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(10),
            top: px(crate::hud::TOP_BAR_H + 26.0),
            padding: UiRect::axes(px(8), px(4)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.07, 0.10, 0.7)),
        children![(label(14.0, Color::srgb(1.0, 0.85, 0.45)), GoalText)],
    ));
    // Funkspruch mittig über der unteren Leiste
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            bottom: px(crate::hud::BOTTOM_PANEL_H + 70.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            Node { padding: UiRect::axes(px(12), px(6)), border_radius: BorderRadius::all(px(6)), max_width: px(760), ..default() },
            BackgroundColor(Color::srgba(0.03, 0.06, 0.08, 0.8)),
            Visibility::Hidden,
            RadioText,
            children![label(16.0, Color::srgb(0.6, 0.95, 0.85))],
        )],
    ));
}

#[derive(Default)]
pub struct MissionState {
    next_radio: usize,
    radio_age: f32,
    marked_start: Option<usize>,
    last_marked: usize,
    saved: bool,
}

/// Ziel prüfen, Funksprüche zeigen, Fortschritt speichern
pub fn mission_tick(
    cfg: Res<MatchConfig>,
    stats: Res<Stats>,
    time: Res<Time>,
    mut outcome: ResMut<Outcome>,
    marked: Query<(), (With<Objective>, With<Health>)>,
    mut goal_text: Query<&mut Text, With<GoalText>>,
    mut radio: Query<(&mut Visibility, &Children), With<RadioText>>,
    mut texts: Query<&mut Text, Without<GoalText>>,
    mut sfx: MessageWriter<crate::audio::PlaySfx>,
    mut st: Local<MissionState>,
) {
    let Some(m) = current(&cfg) else { return };
    let mut message: Option<String> = None;
    if let Some((at, msg)) = m.radio.get(st.next_radio)
        && stats.time >= *at
    {
        message = Some(msg.to_string());
        st.next_radio += 1;
    }

    // Ziel
    let n_marked = marked.iter().count();
    let start = *st.marked_start.get_or_insert(n_marked);
    if *outcome == Outcome::Running {
        let (text, done) = match m.goal {
            Goal::DestroyHq => ("Ziel: VAARs Kern (feindliches HQ) zerstören".to_string(), false),
            Goal::Survive(t) => {
                let left = (t - stats.time).max(0.0) as u32;
                (format!("Ziel: Durchhalten  {}:{:02}", left / 60, left % 60), stats.time >= t)
            }
            Goal::Gather(n) => {
                let have = stats.gathered[PLAYER.0 as usize][0].min(n);
                (format!("Ziel: Ferrit sammeln  {have} / {n}"), have >= n)
            }
            Goal::DestroyMarked => {
                if n_marked < st.last_marked && n_marked > 0 {
                    message = Some(format!("HOLT: Relais ausgefallen! Noch {n_marked}."));
                }
                (format!("Ziel: Relaisstationen zerstören  {} / {start}", start - n_marked), start > 0 && n_marked == 0)
            }
        };
        if let Ok(mut t) = goal_text.single_mut()
            && **t != text
        {
            **t = text;
        }
        if done {
            *outcome = Outcome::Won;
        }
    }

    // Funk: neuer Spruch für neun Sekunden
    st.radio_age += time.delta_secs();
    if let Ok((mut vis, children)) = radio.single_mut() {
        if let Some(msg) = message {
            *vis = Visibility::Inherited;
            if let Some(mut t) = children.first().and_then(|c| texts.get_mut(*c).ok()) {
                **t = msg;
            }
            st.radio_age = 0.0;
            sfx.write(crate::audio::PlaySfx::ui(crate::audio::Sfx::Ack));
        } else if st.radio_age > 9.0 && *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
    }
    st.last_marked = n_marked;
    if *outcome == Outcome::Won && !st.saved {
        st.saved = true;
        save_progress(cfg.mission);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missions_unlock_step_by_step() {
        // jede Mission schaltet mindestens so viel frei wie die vorige
        for w in MISSIONS.windows(2) {
            assert!(w[0].units.iter().all(|u| w[1].units.contains(u)), "{} verliert Einheiten", w[1].title);
            assert!(w[0].buildings.iter().all(|b| w[1].buildings.contains(b)), "{} verliert Gebäude", w[1].title);
            assert!(w[1].units.len() + w[1].buildings.len() > w[0].units.len() + w[0].buildings.len(), "{} schaltet nichts frei", w[1].title);
        }
        // Sammler und Raffinerie gibt es immer, sonst geht die Wirtschaft nicht
        for m in &MISSIONS {
            assert!(m.units.contains(&U::Sammler) && m.enemy_units.contains(&U::Sammler));
            assert!(m.buildings.contains(&B::Raffinerie));
        }
        // Relaismission hat Ziele
        assert!(MISSIONS[2].enemy_structures.iter().filter(|s| s.3).count() == 3);
    }

    #[test]
    fn mission_structures_keep_clear_of_fields_and_bases() {
        for m in &MISSIONS {
            for (k, x, z, _) in m.enemy_structures {
                let p = Vec3::new(*x, 0.0, *z);
                for (f, _, _) in crate::terrain::FIELDS {
                    assert!(p.distance(f) > k.radius() + 5.0, "{} zu nah an einem Feld: {p}", m.title);
                }
                assert!(p.distance(ENEMY_HQ_POS) > 8.0 && p.distance(PLAYER_HQ_POS) > 30.0, "{} schlecht platziert: {p}", m.title);
                assert!(x.abs() < MAP_HALF - 8.0 && z.abs() < MAP_HALF - 8.0);
            }
        }
    }

    #[test]
    fn locked_units_are_refused_or_replaced() {
        use crate::units::{BuildRequest, enqueue_builds};
        let mut app = App::new();
        app.add_message::<BuildRequest>()
            .insert_resource(Stock([[2000, 2000], [2000, 2000]]))
            .insert_resource(Unlocks {
                units: [MISSIONS[0].units.to_vec(), MISSIONS[0].enemy_units.to_vec()],
                buildings: [MISSIONS[0].buildings.to_vec(), MISSIONS[0].enemy_buildings.to_vec()],
            })
            .add_systems(Update, enqueue_builds);
        let hq = |team| (team, Hq { queue: Vec::new(), progress: 0.0 });
        let p = app.world_mut().spawn(hq(PLAYER)).id();
        let e = app.world_mut().spawn(hq(ENEMY)).id();
        for team in [PLAYER, ENEMY] {
            app.world_mut().write_message(BuildRequest { team, kind: UnitKind::Werfer });
            app.world_mut().write_message(BuildRequest { team, kind: UnitKind::Spaeher });
        }
        app.update();
        assert_eq!(app.world().get::<Hq>(p).unwrap().queue, vec![UnitKind::Spaeher], "Spieler darf gesperrte Werfer nicht bauen");
        assert_eq!(app.world().get::<Hq>(e).unwrap().queue, vec![UnitKind::Panzer, UnitKind::Spaeher], "KI baut statt Werfer einen Panzer");
    }
}
