//! VAAR lernt: Die Gegner-KI merkt sich über Partien hinweg, wie der Spieler spielt, und stellt
//! sich beim nächsten Mal darauf ein.
//!
//! - **Beobachten** (`observe`): Was VAAR in dieser Partie gesehen hat (Kampfeinheiten des Spielers,
//!   Türme, Mauern) und wann und von welcher Seite der erste Angriff auf seine Basis kam. Nur
//!   Gesehenes zählt, VAAR schummelt nicht.
//! - **Merken** (`conclude`): Am Ende einer Partie von mindestens vier Minuten fließt das in ein
//!   gleitendes Profil, gespeichert im Browser: Einheitenmischung, Bunkern, früher Angriff,
//!   bevorzugte Flanke. Kurze oder abgebrochene Partien zählen nicht.
//! - **Anpassen** (ai.rs): Das Profil wirkt als Vorwissen für die Konter, das verblasst, sobald VAAR
//!   in der laufenden Partie selbst genug gesehen hat. Gegen frühe Angreifer steht der erste Turm
//!   früher, gegen Bunkerbauer kommen Fabrik und Artillerie eher, Türme zeigen zur gewohnten Flanke.
//!   Die Stärke hängt von der Schwierigkeit ab und lässt nach, wenn der Spieler mehrmals in Folge
//!   verliert. Die Wirtschaft ändert sich nie.
//! - **Sprechen** (`taunts`): VAAR funkt, was es gelernt hat, und kommentiert nur Vorhersagen, die
//!   eingetroffen sind. Am Ende zeigt der Abschlussbildschirm, was VAAR gelernt hat.

use crate::ai::{AiState, Seen};
use crate::audio::{storage_get, storage_set, PlaySfx, Sfx};
use crate::components::*;
use crate::frame::{Difficulty, MatchConfig, Stats};
use crate::hud::BOTTOM_PANEL_H;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

const STORAGE_KEY: &str = "ferrum-nova-vaar";
const VERSION: u32 = 1;
/// Kürzere Partien lehren nichts
pub const MIN_MATCH: f32 = 240.0;
/// Erster Angriff vor dieser Zeit gilt als früh (s)
const RUSH_TIME: f32 = 200.0;

/// Was VAAR über den Spieler weiß (über Partien hinweg)
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct VaarMemory {
    /// Partien, aus denen VAAR gelernt hat
    pub games: u32,
    pub player_wins: u32,
    /// Niederlagen des Spielers in Folge
    pub loss_streak: u32,
    /// Anteile der Kampfeinheiten (Index wie `UnitKind::idx`, Sammler bleibt 0)
    pub mix: [f32; 6],
    /// Anteil der Partien, in denen der Spieler sich eingemauert hat
    pub turtle: f32,
    /// Anteil der Partien mit frühem Angriff
    pub rush: f32,
    /// Gewohnte Flanke: -1 Südost .. 0 Mitte .. 1 Nordwest (quer zur Achse der beiden HQs)
    pub side: f32,
    /// Wirkung der Anpassung (0..1), aus Schwierigkeit und Niederlagenserie; nicht gespeichert
    pub strength: f32,
}

impl Default for VaarMemory {
    fn default() -> Self {
        Self { games: 0, player_wins: 0, loss_streak: 0, mix: [0.0; 6], turtle: 0.0, rush: 0.0, side: 0.0, strength: 1.0 }
    }
}

/// Richtung quer zur Achse der beiden HQs (zeigt nach Nordwest)
fn flank_axis() -> Vec3 {
    let a = flat(PLAYER_HQ_POS - ENEMY_HQ_POS).normalize();
    Vec3::new(a.z, 0.0, -a.x) * -1.0
}

/// Wo liegt ein Punkt quer zur Achse? -1..1
pub fn lateral(p: Vec3) -> f32 {
    flat(p - ENEMY_HQ_POS).normalize_or_zero().dot(flank_axis())
}

pub fn flank_name(side: f32) -> &'static str {
    if side > 0.3 {
        "Nordwest-Flanke"
    } else if side < -0.3 {
        "Suedost-Flanke"
    } else {
        "Mitte"
    }
}

/// Ergebnis einer Partie aus VAARs Sicht
#[derive(Clone, Debug, PartialEq)]
pub struct Lesson {
    pub mix: [f32; 6],
    pub turtle: bool,
    pub rush: bool,
    pub side: Option<f32>,
    pub player_won: bool,
}

impl VaarMemory {
    pub fn load(cfg: &MatchConfig) -> Self {
        let mut m = storage_get(STORAGE_KEY).map(|s| Self::parse(&s)).unwrap_or_default();
        m.set_strength(cfg.difficulty);
        m
    }

    fn set_strength(&mut self, d: Difficulty) {
        let base = match d {
            Difficulty::Leicht => 0.5,
            Difficulty::Normal | Difficulty::Schwer => 1.0,
        };
        // Gnade: nach drei Niederlagen in Folge passt sich VAAR kaum noch an
        let streak = if self.loss_streak >= 3 { 0.4 } else { 1.0 };
        self.strength = base * streak;
    }

    pub fn parse(s: &str) -> Self {
        let mut m = Self::default();
        let get = |k: &str| s.split(';').find_map(|kv| kv.strip_prefix(k).and_then(|v| v.strip_prefix('=')));
        let f = |k: &str| get(k).and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite());
        let u = |k: &str| get(k).and_then(|v| v.parse::<u32>().ok());
        if u("v").is_none_or(|v| v > VERSION) {
            return m;
        }
        m.games = u("games").unwrap_or(0);
        m.player_wins = u("wins").unwrap_or(0).min(m.games);
        m.loss_streak = u("streak").unwrap_or(0);
        if let Some(v) = get("mix") {
            for (i, x) in v.split(',').take(6).enumerate() {
                m.mix[i] = x.parse::<f32>().unwrap_or(0.0).clamp(0.0, 1.0);
            }
        }
        m.turtle = f("turtle").unwrap_or(0.0).clamp(0.0, 1.0);
        m.rush = f("rush").unwrap_or(0.0).clamp(0.0, 1.0);
        m.side = f("side").unwrap_or(0.0).clamp(-1.0, 1.0);
        m
    }

    pub fn serialize(&self) -> String {
        let mix: Vec<String> = self.mix.iter().map(|x| format!("{x:.3}")).collect();
        format!(
            "v={VERSION};games={};wins={};streak={};mix={};turtle={:.3};rush={:.3};side={:.3}",
            self.games,
            self.player_wins,
            self.loss_streak,
            mix.join(","),
            self.turtle,
            self.rush,
            self.side
        )
    }

    /// Eine Partie einarbeiten: anfangs zählt jede Partie viel, später gleitet das Profil
    pub fn learn(&mut self, l: &Lesson) {
        self.games += 1;
        let a = (1.0 / self.games as f32).max(0.35);
        let mix_total: f32 = l.mix.iter().sum();
        if mix_total > 0.0 {
            for i in 0..6 {
                self.mix[i] += (l.mix[i] / mix_total - self.mix[i]) * a;
            }
        }
        self.turtle += (l.turtle as u8 as f32 - self.turtle) * a;
        self.rush += (l.rush as u8 as f32 - self.rush) * a;
        if let Some(s) = l.side {
            self.side += (s - self.side) * a;
        }
        if l.player_won {
            self.player_wins += 1;
            self.loss_streak = 0;
        } else {
            self.loss_streak += 1;
        }
    }

    /// Vorwissen für die Konter: erwartete Einheiten, verblasst mit dem, was VAAR schon gesehen hat
    pub fn prior(&self, seen_total: usize) -> [f32; 6] {
        if self.games == 0 {
            return [0.0; 6];
        }
        let w = 4.0 * (-(seen_total as f32) / 8.0).exp() * self.strength;
        let mut p = self.mix;
        p[UnitKind::Sammler.idx()] = 0.0;
        p.map(|x| x * w)
    }

    fn confident(&self) -> bool {
        self.games >= 1 && self.strength >= 0.5
    }

    /// Greift der Spieler meist früh an?
    pub fn expects_rush(&self) -> bool {
        self.confident() && self.rush > 0.5
    }

    /// Mauert er sich meist ein?
    pub fn expects_turtle(&self) -> bool {
        self.confident() && self.turtle > 0.5
    }

    /// Gewohnte Angriffsseite, falls deutlich
    pub fn expected_side(&self) -> Option<f32> {
        (self.confident() && self.side.abs() > 0.3).then_some(self.side)
    }

    /// Richtung von VAARs HQ, aus der ein Angriff erwartet wird
    pub fn expected_front(&self) -> Option<Vec3> {
        let to_player = flat(PLAYER_HQ_POS - ENEMY_HQ_POS).normalize();
        self.expected_side().map(|s| (to_player + flank_axis() * s).normalize())
    }

    /// Lieblingseinheit des Spielers (Anteil), falls deutlich
    pub fn favourite(&self) -> Option<(UnitKind, f32)> {
        UnitKind::ALL
            .into_iter()
            .filter(|k| *k != UnitKind::Sammler)
            .map(|k| (k, self.mix[k.idx()]))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|(_, p)| *p >= 0.35)
    }
}

/// Beobachtungen dieser Partie
#[derive(Resource, Default)]
pub struct VaarObs {
    units: HashMap<Entity, UnitKind>,
    defenses: HashSet<Entity>,
    walls: HashSet<Entity>,
    pub first_strike: Option<f32>,
    side_sum: f32,
    side_n: u32,
    done: bool,
    timer: f32,
}

impl VaarObs {
    pub fn lesson(&self, player_won: bool) -> Lesson {
        let mut mix = [0.0; 6];
        for k in self.units.values() {
            mix[k.idx()] += 1.0;
        }
        Lesson {
            mix,
            turtle: self.defenses.len() + self.walls.len() / 8 >= 3,
            rush: self.first_strike.is_some_and(|t| t < RUSH_TIME),
            side: (self.side_n > 0).then(|| (self.side_sum / self.side_n as f32).clamp(-1.0, 1.0)),
            player_won,
        }
    }

    fn scouted(&self, k: UnitKind) -> usize {
        self.units.values().filter(|u| **u == k).count()
    }
}

/// Was VAAR sieht, mitschreiben (zweimal pro Sekunde)
pub fn observe(time: Res<Time>, ai: Res<AiState>, mut obs: ResMut<VaarObs>, outcome: Res<Outcome>) {
    if *outcome != Outcome::Running {
        return;
    }
    obs.timer -= time.delta_secs();
    if obs.timer > 0.0 {
        return;
    }
    obs.timer = 0.5;
    for (e, s) in &ai.intel {
        match s.what {
            Seen::Unit(k) if k != UnitKind::Sammler => {
                obs.units.insert(*e, k);
            }
            Seen::Structure(BuildingKind::Geschuetzturm | BuildingKind::Raketenturm) => {
                obs.defenses.insert(*e);
            }
            Seen::Structure(BuildingKind::Mauer | BuildingKind::Tor) => {
                obs.walls.insert(*e);
            }
            _ => {}
        }
    }
    if let Some((p, _)) = ai.threat {
        obs.first_strike.get_or_insert(ai.elapsed);
        obs.side_sum += lateral(p);
        obs.side_n += 1;
    }
}

/// Partie vorbei: lernen und speichern
pub fn conclude(
    outcome: Res<Outcome>,
    stats: Res<Stats>,
    mut obs: ResMut<VaarObs>,
    mut mem: ResMut<VaarMemory>,
    mut report: Query<&mut Text, With<VaarReport>>,
) {
    if *outcome == Outcome::Running || obs.done {
        return;
    }
    obs.done = true;
    let player_won = *outcome == Outcome::Won;
    let text = if stats.time < MIN_MATCH {
        "VAAR: Zu kurz, um daraus zu lernen.".to_string()
    } else {
        let lesson = obs.lesson(player_won);
        mem.learn(&lesson);
        storage_set(STORAGE_KEY, &mem.serialize());
        report_text(&lesson, obs.first_strike, &mem)
    };
    if let Ok(mut t) = report.single_mut() {
        **t = text;
    }
}

fn report_text(l: &Lesson, first: Option<f32>, mem: &VaarMemory) -> String {
    let mut parts = Vec::new();
    let total: f32 = l.mix.iter().sum();
    if total > 0.0
        && let Some(k) = UnitKind::ALL.into_iter().filter(|k| *k != UnitKind::Sammler).max_by(|a, b| l.mix[a.idx()].total_cmp(&l.mix[b.idx()]))
    {
        parts.push(format!("{} % {}", (l.mix[k.idx()] / total * 100.0).round(), k.name()));
    }
    if let Some(s) = l.side {
        parts.push(format!("Angriffe ueber die {}", flank_name(s)));
    }
    if let Some(t) = first {
        let t = t as u32;
        parts.push(format!("erster Angriff nach {}:{:02}", t / 60, t % 60));
    }
    if l.turtle {
        parts.push("eingemauert".into());
    }
    if parts.is_empty() {
        parts.push("kaum etwas gesehen".into());
    }
    format!("VAAR hat gelernt: {}   (Gefechte im Gedaechtnis: {})", parts.join("  |  "), mem.games)
}

/// Zeile im Abschlussbildschirm (frame.rs)
#[derive(Component)]
pub struct VaarReport;

/// Funkzeile für VAAR
#[derive(Component)]
pub struct VaarRadio;

pub fn setup_vaar_hud(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            bottom: px(BOTTOM_PANEL_H + 112.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            Node { padding: UiRect::axes(px(12), px(6)), border_radius: BorderRadius::all(px(6)), max_width: px(760), ..default() },
            BackgroundColor(Color::srgba(0.12, 0.02, 0.03, 0.82)),
            Visibility::Hidden,
            VaarRadio,
            children![(Text::new(""), TextFont { font_size: FontSize::Px(16.0), ..default() }, TextColor(Color::srgb(1.0, 0.45, 0.40)))],
        )],
    ));
}

#[derive(Default)]
pub struct TauntState {
    said: u32,
    last: f32,
    age: f32,
    shown: bool,
}

const T_INTRO: u32 = 1;
const T_PREDICT: u32 = 2;
const T_ADAPT: u32 = 4;
const T_DARK: u32 = 8;

/// VAAR funkt: was es gelernt hat, eingetroffene Vorhersagen, Anpassung an Gesehenes, die Finsternis
pub fn taunts(
    time: Res<Time>,
    stats: Res<Stats>,
    outcome: Res<Outcome>,
    mem: Res<VaarMemory>,
    obs: Res<VaarObs>,
    eclipse: Option<Res<crate::halkyon::Eclipse>>,
    mut radio: Query<(&mut Visibility, &Children), With<VaarRadio>>,
    mut texts: Query<&mut Text>,
    mut sfx: MessageWriter<PlaySfx>,
    mut st: Local<TauntState>,
) {
    let Ok((mut vis, children)) = radio.single_mut() else { return };
    st.age += time.delta_secs();
    if st.shown && st.age > 8.0 {
        *vis = Visibility::Hidden;
        st.shown = false;
    }
    if *outcome != Outcome::Running {
        return;
    }
    let t = stats.time;
    let ready = t - st.last > 45.0 || st.said == 0;
    let mut say: Option<(u32, String)> = None;
    if st.said & T_INTRO == 0 && t > 20.0 {
        say = Some((
            T_INTRO,
            match (mem.games, mem.favourite()) {
                (0, _) => "VAAR: Ein neuer Kommandant. Ich beobachte. Ich lerne.".into(),
                (1, _) => "VAAR: Wir kennen uns. Ich habe mir unser letztes Gefecht gemerkt.".into(),
                (n, Some((k, p))) => format!("VAAR: Analyse von {n} Gefechten abgeschlossen. Du setzt zu {} % auf {}. Ich habe vorgesorgt.", (p * 100.0).round(), k.name()),
                (n, None) => format!("VAAR: Analyse von {n} Gefechten abgeschlossen. Du bist schwer zu lesen. Noch."),
            },
        ));
    } else if st.said & T_PREDICT == 0 && mem.games >= 2 && obs.first_strike.is_some() {
        // nur eingetroffene Vorhersagen kommentieren
        let side_hit = mem.expected_side().filter(|s| obs.side_n > 0 && (obs.side_sum / obs.side_n as f32) * s > 0.1);
        let rush_hit = mem.expects_rush() && obs.first_strike.is_some_and(|f| f < RUSH_TIME);
        if let Some(s) = side_hit {
            say = Some((T_PREDICT, format!("VAAR: Wie erwartet. Du kommst ueber die {}.", flank_name(s))));
        } else if rush_hit {
            say = Some((T_PREDICT, "VAAR: Ein frueher Angriff. Wie beim letzten Mal. Meine Tuerme stehen schon.".into()));
        } else {
            st.said |= T_PREDICT;
        }
    } else if st.said & T_ADAPT == 0 && ready {
        // nur, was VAAR in dieser Partie selbst gesehen hat
        if let Some(k) = [UnitKind::Panzer, UnitKind::Bollwerk, UnitKind::Werfer, UnitKind::Haubitze, UnitKind::Spaeher]
            .into_iter()
            .find(|k| obs.scouted(*k) >= 6)
        {
            say = Some((T_ADAPT, format!("VAAR: {} {}. Ich passe meine Produktion an.", obs.scouted(k), k.name())));
        }
    } else if st.said & T_DARK == 0 && ready && eclipse.is_some_and(|e| e.shade > 0.6) {
        say = Some((T_DARK, "VAAR: Halkyon verdeckt die Sonne. Deine Sensoren sind blind. Meine nicht.".into()));
    }
    if let Some((flag, msg)) = say {
        st.said |= flag;
        st.last = t;
        st.age = 0.0;
        st.shown = true;
        *vis = Visibility::Inherited;
        if let Some(mut txt) = children.first().and_then(|c| texts.get_mut(*c).ok()) {
            **txt = msg;
        }
        sfx.write(PlaySfx::ui(Sfx::Ack));
    }
}

/// Gedächtnis löschen (Knopf im Hauptmenü)
pub fn forget(mem: &mut VaarMemory) {
    let strength = mem.strength;
    *mem = VaarMemory { strength, ..default() };
    storage_set(STORAGE_KEY, &mem.serialize());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lesson(panzer: f32, werfer: f32, won: bool) -> Lesson {
        let mut mix = [0.0; 6];
        mix[UnitKind::Panzer.idx()] = panzer;
        mix[UnitKind::Werfer.idx()] = werfer;
        Lesson { mix, turtle: false, rush: true, side: Some(0.8), player_won: won }
    }

    #[test]
    fn memory_learns_blends_and_survives_storage() {
        let mut m = VaarMemory::default();
        assert_eq!(m.prior(0), [0.0; 6], "ohne Erfahrung kein Vorwissen");
        m.learn(&lesson(6.0, 2.0, false));
        assert!((m.mix[UnitKind::Panzer.idx()] - 0.75).abs() < 1e-4);
        assert!(m.expects_rush() && m.expected_side() == Some(0.8));
        assert_eq!(m.favourite().map(|f| f.0), Some(UnitKind::Panzer));
        // eine abweichende Partie verschiebt das Profil nur teilweise
        m.learn(&lesson(0.0, 4.0, true));
        let p = m.mix[UnitKind::Panzer.idx()];
        assert!(p > 0.3 && p < 0.75, "{p}");
        assert_eq!((m.games, m.player_wins, m.loss_streak), (2, 1, 0));
        // Vorwissen verblasst mit eigener Sicht
        assert!(m.prior(0)[UnitKind::Panzer.idx()] > 4.0 * m.prior(24)[UnitKind::Panzer.idx()]);
        // Speichern und Laden
        let back = VaarMemory::parse(&m.serialize());
        assert_eq!(back.games, 2);
        assert!((back.mix[UnitKind::Werfer.idx()] - m.mix[UnitKind::Werfer.idx()]).abs() < 1e-3);
        assert!((back.side - m.side).abs() < 1e-3);
        // Unsinn oder fremde Version: leeres Gedächtnis
        assert_eq!(VaarMemory::parse("v=99;games=5").games, 0);
        assert_eq!(VaarMemory::parse("kaputt").games, 0);
    }

    #[test]
    fn adaptation_softens_after_a_losing_streak_and_on_easy() {
        let mut m = VaarMemory::default();
        for _ in 0..3 {
            m.learn(&lesson(5.0, 0.0, false));
        }
        m.set_strength(Difficulty::Normal);
        assert!(m.strength < 0.5 && !m.expects_rush(), "Gnade nach drei Niederlagen");
        m.learn(&lesson(5.0, 0.0, true));
        m.set_strength(Difficulty::Normal);
        assert_eq!(m.strength, 1.0);
        m.set_strength(Difficulty::Leicht);
        assert!(m.prior(0)[UnitKind::Panzer.idx()] < 2.5);
    }

    #[test]
    fn flanks_are_named_from_vaars_point_of_view() {
        let nw = ENEMY_HQ_POS + flank_axis() * 20.0;
        assert!(lateral(nw) > 0.9 && flank_name(lateral(nw)) == "Nordwest-Flanke");
        assert!(flank_axis().x < 0.0 && flank_axis().z < 0.0, "Nordwest: -x, -z");
        assert!(lateral(PLAYER_HQ_POS).abs() < 1e-4);
    }
}
