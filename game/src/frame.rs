//! Spielrahmen: Hauptmenü, Schwierigkeit, Pause, Endbildschirm mit Statistik und Neustart.
//!
//! Neustart und Missionswechsel laden die Seite neu (im Browser) bzw. starten das Programm neu
//! (nativ). Die Wahl steht dabei in der Adresse (`?d=schwer&auto=1`), nicht im Browser-Speicher,
//! weil eingebettete Seiten (iframe) darauf oft keinen Zugriff haben. So beginnt jede Partie mit
//! frischem Zustand, ohne dass jede Ressource einzeln zurückgesetzt werden muss.

use crate::ai::AiState;
use crate::audio::{PlaySfx, Sfx, SoundPrefs};
use crate::components::*;
use crate::construction::Placing;
use bevy::input::ButtonState;
use bevy::input::mouse::MouseButtonInput;
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Difficulty {
    Leicht,
    #[default]
    Normal,
    Schwer,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Leicht, Difficulty::Normal, Difficulty::Schwer];

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Leicht => "Leicht",
            Difficulty::Normal => "Normal",
            Difficulty::Schwer => "Schwer",
        }
    }
    fn code(self) -> &'static str {
        match self {
            Difficulty::Leicht => "leicht",
            Difficulty::Normal => "normal",
            Difficulty::Schwer => "schwer",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Difficulty::Leicht => "Der Gegner sammelt langsamer und greift spaet an.",
            Difficulty::Normal => "Ausgewogen.",
            Difficulty::Schwer => "Der Gegner sammelt schneller und greift frueh an.",
        }
    }
    /// Faktor auf jede Ladung, die die Gegner-KI abliefert
    pub fn ai_income(self) -> f32 {
        match self {
            Difficulty::Leicht => 0.7,
            Difficulty::Normal => 1.0,
            Difficulty::Schwer => 1.35,
        }
    }
    /// Erster Angriff (Sekunden) und Größe der ersten Welle
    fn first_attack(self) -> (f32, usize) {
        match self {
            Difficulty::Leicht => (240.0, 4),
            Difficulty::Normal => (150.0, 5),
            Difficulty::Schwer => (110.0, 6),
        }
    }
}

/// Wie diese Partie gestartet wurde
#[derive(Resource, Clone, Debug, Default)]
pub struct MatchConfig {
    pub difficulty: Difficulty,
    /// 0 = freies Gefecht, ab 1 Kampagnenmission
    pub mission: u8,
    /// direkt losspielen statt das Hauptmenü zu zeigen
    pub autostart: bool,
}

impl MatchConfig {
    fn query(&self) -> String {
        let mut q = format!("?d={}", self.difficulty.code());
        if self.mission > 0 {
            q += &format!("&m={}", self.mission);
        }
        if self.autostart {
            q += "&auto=1";
        }
        q
    }

    /// Liest `d=…`, `m=…`, `auto=1` aus einer Adresse oder aus Programmargumenten
    pub fn parse<'a>(parts: impl Iterator<Item = &'a str>) -> Self {
        let mut cfg = MatchConfig::default();
        for p in parts {
            let p = p.trim_start_matches(['?', '-']);
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            match k {
                "d" => {
                    if let Some(d) = Difficulty::ALL.into_iter().find(|d| d.code() == v) {
                        cfg.difficulty = d;
                    }
                }
                "m" => cfg.mission = v.parse().unwrap_or(0),
                "auto" => cfg.autostart = v != "0",
                _ => {}
            }
        }
        cfg
    }

    /// Einnahmen-Faktor der Gegner-KI: Schwierigkeit mal Mission
    pub fn ai_income(&self) -> f32 {
        self.difficulty.ai_income() * crate::campaign::current(self).map_or(1.0, |m| m.enemy_income)
    }

    /// Einstellungen beim Programmstart (Browser: Adresse, nativ: Argumente)
    pub fn load() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            let Some(win) = web_sys::window() else { return Self::default() };
            let search = win.location().search().unwrap_or_default();
            let cfg = Self::parse(search.trim_start_matches('?').split('&'));
            // Adresse wieder säubern, damit ein Neuladen von Hand (oder das Live-Neuladen
            // beim Entwickeln) nicht wieder sofort losspielt
            if !search.is_empty()
                && let Ok(history) = win.history()
            {
                let path = win.location().pathname().unwrap_or_default();
                let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&path));
            }
            cfg
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let args: Vec<String> = std::env::args().skip(1).collect();
            Self::parse(args.iter().map(|s| s.as_str()))
        }
    }
}

/// Neu beginnen mit diesen Einstellungen
fn relaunch(cfg: &MatchConfig) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(win) = web_sys::window() else { return };
        if let Some(doc) = win.document()
            && let Some(el) = doc.get_element_by_id("loading")
        {
            let _ = el.set_attribute("style", "");
        }
        let path = win.location().pathname().unwrap_or_default();
        let _ = win.location().replace(&format!("{path}{}", cfg.query()));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // winit kann im selben Prozess kein zweites Fenster-System starten: neu ausführen
        if let Ok(exe) = std::env::current_exe() {
            let args = cfg.query().trim_start_matches('?').split('&').map(|a| format!("--{a}")).collect::<Vec<_>>();
            let _ = std::process::Command::new(exe).args(args).spawn();
        }
        std::process::exit(0);
    }
}

/// Wo im Ablauf das Spiel gerade steht
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Phase {
    #[default]
    Menu,
    /// Einsatzbesprechung vor einer Kampagnenmission
    Briefing,
    Playing,
    Paused,
}

pub fn playing(phase: Option<Res<Phase>>, outcome: Res<Outcome>) -> bool {
    phase.is_none_or(|p| *p == Phase::Playing) && *outcome == Outcome::Running
}

/// Zahlen für den Endbildschirm, je Team
#[derive(Resource, Default, Debug)]
pub struct Stats {
    pub time: f32,
    pub units_built: [u32; 2],
    pub units_lost: [u32; 2],
    pub structures_built: [u32; 2],
    pub structures_lost: [u32; 2],
    pub gathered: [[u32; 2]; 2],
    started: bool,
}

/// Zählt mit: neue Einheiten, fertige Gebäude, Verluste. Läuft direkt vor `units::deaths`.
pub fn track_stats(
    time: Res<Time>,
    mut stats: ResMut<Stats>,
    new_units: Query<&Team, Added<Unit>>,
    dying: Query<(&Team, &Health, Has<Unit>, Has<Building>, Has<Wall>)>,
    mut finished: RemovedComponents<Construction>,
    buildings: Query<&Team, (With<Building>, Without<Wall>)>,
) {
    stats.time += time.delta_secs();
    let fresh: Vec<Team> = new_units.iter().copied().collect();
    // Die Startarmeen zählen nicht als gebaut
    if stats.started {
        for t in fresh {
            stats.units_built[t.0 as usize] += 1;
        }
    }
    stats.started = true;
    for e in finished.read() {
        if let Ok(t) = buildings.get(e) {
            stats.structures_built[t.0 as usize] += 1;
        }
    }
    for (t, h, unit, building, wall) in &dying {
        if h.hp > 0.0 {
            continue;
        }
        if unit {
            stats.units_lost[t.0 as usize] += 1;
        } else if building && !wall {
            stats.structures_lost[t.0 as usize] += 1;
        }
    }
}

// ---------------------------------------------------------------- Oberfläche

const OVERLAY: Color = Color::srgba(0.02, 0.03, 0.05, 0.80);
const CARD: Color = Color::srgba(0.06, 0.08, 0.11, 0.94);
const BTN: Color = Color::srgb(0.14, 0.20, 0.28);
const BTN_HOVER: Color = Color::srgb(0.20, 0.30, 0.42);
const BTN_ON: Color = Color::srgb(0.22, 0.45, 0.38);
const BTN_LOCKED: Color = Color::srgb(0.09, 0.10, 0.12);
const TEXT: Color = Color::srgb(0.88, 0.92, 0.96);
const DIM: Color = Color::srgb(0.62, 0.70, 0.78);
const ACCENT: Color = Color::srgb(0.55, 0.85, 1.0);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum FrameButton {
    Start,
    Diff(Difficulty),
    Help,
    Resume,
    Again,
    Menu,
    Sound,
    Music,
    /// Zeitsprung 2026 / 1996
    Era,
    /// Kampagnen-Auswahl ein-/ausblenden
    Campaign,
    /// Mission wählen (1-basiert)
    Mission(u8),
    StartMission,
    NextMission,
}

#[derive(Component)]
pub struct CampaignPanel;
#[derive(Component)]
pub struct BriefingPanel;
/// Text unter der Statistik am Spielende (Ausgang der Mission)
#[derive(Component)]
pub struct EndStory;
/// Knopf „Nächste Mission“ (nur nach einem Missionssieg)
#[derive(Component)]
pub struct NextButton;

#[derive(Resource, Default)]
pub struct CampaignShown(pub bool);

/// Beschriftung eines Ein/Aus-Knopfs (Ton, Musik)
#[derive(Component)]
pub struct ToggleCaption(FrameButton);

#[derive(Component)]
pub struct MenuPanel;
#[derive(Component)]
pub struct PausePanel;
#[derive(Component)]
pub struct EndPanel;
#[derive(Component)]
pub struct HelpPanel;
#[derive(Component)]
pub struct DiffHint;
#[derive(Component)]
pub struct EndTitle;
/// Spalte der Statistik-Tabelle (0 = Beschriftung, 1 = Spieler, 2 = Gegner)
#[derive(Component)]
pub struct EndColumn(u8);

#[derive(Resource, Default)]
pub struct HelpShown(pub bool);

/// Gesperrter Knopf (noch nicht freigeschaltete Mission)
#[derive(Component)]
pub struct Locked;

fn text(s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(s), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

fn button(action: FrameButton, caption: &str, width: f32) -> impl Bundle {
    (
        action,
        RelativeCursorPosition::default(),
        Node {
            width: px(width),
            height: px(42),
            margin: UiRect::all(px(5)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(6)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor::all(Color::srgb(0.35, 0.5, 0.65)),
        BackgroundColor(BTN),
        children![text(caption, 17.0, TEXT)],
    )
}

fn toggle(action: FrameButton, width: f32) -> impl Bundle {
    (
        action,
        RelativeCursorPosition::default(),
        Node {
            width: px(width),
            height: px(34),
            margin: UiRect::all(px(5)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(6)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor::all(Color::srgb(0.3, 0.4, 0.5)),
        BackgroundColor(BTN),
        children![(text("", 14.0, TEXT), ToggleCaption(action))],
    )
}

fn sound_row() -> impl Bundle {
    (
        Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(8)), ..default() },
        children![toggle(FrameButton::Sound, 135.0), toggle(FrameButton::Music, 135.0)],
    )
}

fn era_row() -> impl Bundle {
    (Node { flex_direction: FlexDirection::Row, ..default() }, children![toggle(FrameButton::Era, 280.0)])
}

/// Bildschirmfüllende, abgedunkelte Ebene mit einer Karte in der Mitte
fn overlay(marker: impl Bundle, z: i32) -> impl Bundle {
    (
        marker,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(OVERLAY),
        GlobalZIndex(z),
        Visibility::Hidden,
    )
}

fn card() -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            padding: UiRect::axes(px(36), px(26)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(CARD),
        BorderColor::all(Color::srgb(0.22, 0.32, 0.42)),
    )
}

const HELP: &str = "\
Linksklick: auswaehlen   |   Ziehen: Rahmen aufziehen   |   Doppelklick: alle dieses Typs
Rechtsklick: bewegen / angreifen   |   Strg: Angriffsmarsch   |   Alt: Patrouille   |   Shift: anhaengen
L: Ein-Tasten-Modus (alles mit Linksklick, fuer Trackpads)
R: Stopp   |   T: Stellung halten   |   F: ganze Armee   |   Strg + 1-9: Gruppe speichern
Kamera: WASD / Pfeile / Mitteltaste / Wischen   |   Q / E drehen   |   Rad / Spreizen zoomen
Leertaste: zum letzten Angriff   |   Pos1: zur Basis   |   P / Esc: Pause   |   Z: Zeitsprung 1996 / 2026
Einheiten: X C V B N M (am HQ)   |   Gebaeude: G H J U K I";

pub fn setup_frame(mut commands: Commands, cfg: Res<MatchConfig>) {
    // Hauptmenü
    commands.spawn((overlay(MenuPanel, 50), children![(
        card(),
        children![
            text("FERRUM NOVA", 58.0, ACCENT),
            (text("Echtzeitstrategie", 16.0, DIM), Node { margin: UiRect::bottom(px(22)), ..default() }),
            button(FrameButton::Start, "Gefecht starten", 280.0),
            button(FrameButton::Campaign, "Kampagne", 280.0),
            (text("Schwierigkeit", 14.0, DIM), Node { margin: UiRect::top(px(16)), ..default() }),
            (
                Node { flex_direction: FlexDirection::Row, ..default() },
                children![
                    button(FrameButton::Diff(Difficulty::Leicht), "Leicht", 120.0),
                    button(FrameButton::Diff(Difficulty::Normal), "Normal", 120.0),
                    button(FrameButton::Diff(Difficulty::Schwer), "Schwer", 120.0),
                ],
            ),
            (text(cfg.difficulty.hint(), 13.0, DIM), DiffHint, Node { margin: UiRect::bottom(px(16)), ..default() }),
            button(FrameButton::Help, "Steuerung", 280.0),
            sound_row(),
            era_row(),
        ],
    )]));

    // Pause
    commands.spawn((overlay(PausePanel, 50), children![(
        card(),
        children![
            (text("PAUSE", 44.0, ACCENT), Node { margin: UiRect::bottom(px(14)), ..default() }),
            button(FrameButton::Resume, "Weiter  [P]", 260.0),
            button(FrameButton::Help, "Steuerung", 260.0),
            button(FrameButton::Again, "Neu beginnen", 260.0),
            button(FrameButton::Menu, "Hauptmenue", 260.0),
            sound_row(),
            era_row(),
        ],
    )]));

    // Spielende
    commands.spawn((overlay(EndPanel, 50), children![(
        card(),
        children![
            (text("", 46.0, ACCENT), EndTitle, Node { margin: UiRect::bottom(px(16)), ..default() }),
            (
                Node { flex_direction: FlexDirection::Row, column_gap: px(28), margin: UiRect::bottom(px(18)), ..default() },
                children![
                    (text("", 16.0, DIM), EndColumn(0)),
                    (text("", 16.0, TEXT), EndColumn(1), TextLayout::justify(Justify::Right)),
                    (text("", 16.0, TEXT), EndColumn(2), TextLayout::justify(Justify::Right)),
                ],
            ),
            (text("", 16.0, TEXT), EndStory, TextLayout::justify(Justify::Center), Node { margin: UiRect::bottom(px(14)), max_width: px(640), ..default() }),
            (
                Node { flex_direction: FlexDirection::Row, ..default() },
                children![
                    (button(FrameButton::NextMission, "Naechste Mission", 220.0), NextButton),
                    button(FrameButton::Again, "Nochmal", 200.0),
                    button(FrameButton::Menu, "Hauptmenue", 200.0),
                ],
            ),
        ],
    )]));

    // Kampagne: Auswahl der Missionen (freigeschaltet ist, was nach der letzten geschafften kommt)
    let done = crate::campaign::progress();
    commands.spawn(overlay(CampaignPanel, 55)).with_children(|root| {
        root.spawn(card()).with_children(|c| {
            c.spawn((text("KAMPAGNE", 40.0, ACCENT), Node { margin: UiRect::bottom(px(4)), ..default() }));
            c.spawn((text("Der Aufstand von VAAR", 16.0, DIM), Node { margin: UiRect::bottom(px(16)), ..default() }));
            for (i, m) in crate::campaign::MISSIONS.iter().enumerate() {
                let n = i as u8 + 1;
                let open = n <= done + 1;
                let caption = if open { format!("{n}   {}", m.title) } else { format!("{n}   (gesperrt)") };
                let mut b = c.spawn(button(FrameButton::Mission(n), &caption, 300.0));
                if !open {
                    b.insert(Locked);
                }
            }
            c.spawn(button(FrameButton::Campaign, "Zurueck", 300.0));
        });
    });

    // Einsatzbesprechung
    if let Some(m) = crate::campaign::current(&cfg) {
        let n = cfg.mission as usize;
        let prev = n.checked_sub(2).map(|i| &crate::campaign::MISSIONS[i]);
        let mut new: Vec<&str> = m.units.iter().filter(|u| prev.is_none_or(|p| !p.units.contains(u))).map(|u| u.short_name()).collect();
        new.extend(m.buildings.iter().filter(|b| prev.is_none_or(|p| !p.buildings.contains(b))).map(|b| b.short_name()));
        let unlocked = format!("Verfuegbar neu: {}", new.join(", "));
        commands.spawn(overlay(BriefingPanel, 50)).with_children(|root| {
            root.spawn(card()).with_children(|c| {
                c.spawn(text(format!("MISSION {n}"), 18.0, DIM));
                c.spawn((text(m.title.to_uppercase(), 44.0, ACCENT), Node { margin: UiRect::bottom(px(16)), ..default() }));
                c.spawn((text(m.briefing, 16.0, TEXT), Node { max_width: px(720), margin: UiRect::bottom(px(14)), ..default() }));
                c.spawn((text(unlocked, 14.0, Color::srgb(1.0, 0.85, 0.45)), Node { margin: UiRect::bottom(px(16)), ..default() }));
                c.spawn(Node { flex_direction: FlexDirection::Row, ..default() }).with_children(|row| {
                    row.spawn(button(FrameButton::StartMission, "Mission starten", 240.0));
                    row.spawn(button(FrameButton::Menu, "Hauptmenue", 200.0));
                });
            });
        });
    }

    // Steuerung (über Menü und Pause)
    commands.spawn((
        HelpPanel,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            bottom: px(30),
            justify_content: JustifyContent::Center,
            ..default()
        },
        GlobalZIndex(60),
        Visibility::Hidden,
        children![(
            card(),
            children![text(HELP, 14.0, TEXT)],
        )],
    ));
}

/// Startet die Partie: Schwierigkeit an die Gegner-KI geben, Zeit laufen lassen
fn begin(cfg: &MatchConfig, ai: &mut AiState, phase: &mut Phase) {
    let (first, wave) = cfg.difficulty.first_attack();
    ai.next_attack = first * crate::campaign::current(cfg).map_or(1.0, |m| m.attack_delay);
    ai.wave_size = wave;
    *phase = Phase::Playing;
}

/// Direkt losspielen, wenn die Adresse das verlangt (Neustart aus dem Spiel)
pub fn autostart(cfg: Res<MatchConfig>, mut ai: ResMut<AiState>, mut phase: ResMut<Phase>) {
    if cfg.autostart {
        begin(&cfg, &mut ai, &mut phase);
    } else if crate::campaign::current(&cfg).is_some() {
        *phase = Phase::Briefing;
    }
}

pub fn frame_buttons(
    mut clicks: MessageReader<MouseButtonInput>,
    mut q: Query<(&FrameButton, &RelativeCursorPosition, &InheritedVisibility, &mut BackgroundColor, Has<Locked>)>,
    mut cfg: ResMut<MatchConfig>,
    mut phase: ResMut<Phase>,
    mut ai: ResMut<AiState>,
    mut help: ResMut<HelpShown>,
    mut hint: Query<&mut Text, With<DiffHint>>,
    mut captions: Query<(&ToggleCaption, &mut Text), Without<DiffHint>>,
    mut prefs: ResMut<SoundPrefs>,
    mut sfx: MessageWriter<PlaySfx>,
    mut campaign: ResMut<CampaignShown>,
    mut era: ResMut<crate::era::Era>,
) {
    for (c, mut t) in &mut captions {
        let (name, on) = match c.0 {
            FrameButton::Sound => ("Ton", prefs.sfx),
            FrameButton::Era => ("", false),
            _ => ("Musik", prefs.music),
        };
        let want = if c.0 == FrameButton::Era {
            format!("Zeitsprung (Z): {}", era.year())
        } else {
            format!("{name}: {}", if on { "an" } else { "aus" })
        };
        if **t != want {
            **t = want;
        }
    }
    let pressed = clicks.read().any(|c| c.button == MouseButton::Left && c.state == ButtonState::Pressed);
    let mut action = None;
    for (b, rel, vis, mut bg, locked) in &mut q {
        if !vis.get() {
            continue;
        }
        if locked {
            *bg = BTN_LOCKED.into();
            continue;
        }
        let over = rel.cursor_over();
        if over && pressed {
            action = Some(*b);
        }
        let on = matches!(b, FrameButton::Diff(d) if *d == cfg.difficulty) || (*b == FrameButton::Help && help.0)
            || (*b == FrameButton::Sound && prefs.sfx)
            || (*b == FrameButton::Music && prefs.music)
            || (*b == FrameButton::Era && *era == crate::era::Era::Retro);
        *bg = if on { BTN_ON } else if over { BTN_HOVER } else { BTN }.into();
    }
    let Some(action) = action else { return };
    sfx.write(PlaySfx::ui(Sfx::UiClick));
    match action {
        FrameButton::Start => {
            help.0 = false;
            let c = cfg.clone();
            begin(&c, &mut ai, &mut phase);
        }
        FrameButton::Diff(d) => {
            cfg.difficulty = d;
            if let Ok(mut t) = hint.single_mut() {
                **t = d.hint().into();
            }
        }
        FrameButton::Help => help.0 = !help.0,
        FrameButton::Resume => {
            help.0 = false;
            *phase = Phase::Playing;
        }
        FrameButton::Again => relaunch(&MatchConfig { autostart: true, ..cfg.clone() }),
        FrameButton::Menu => relaunch(&MatchConfig { autostart: false, mission: 0, ..cfg.clone() }),
        FrameButton::Sound => prefs.sfx = !prefs.sfx,
        FrameButton::Music => prefs.music = !prefs.music,
        FrameButton::Era => *era = era.toggled(),
        FrameButton::Campaign => campaign.0 = !campaign.0,
        FrameButton::Mission(n) => relaunch(&MatchConfig { mission: n, autostart: false, ..cfg.clone() }),
        FrameButton::StartMission => {
            let c = cfg.clone();
            begin(&c, &mut ai, &mut phase);
        }
        FrameButton::NextMission => relaunch(&MatchConfig { mission: cfg.mission + 1, autostart: false, ..cfg.clone() }),
    }
}

/// P oder Esc: Pause (Esc nur, wenn nicht gerade ein Gebäude platziert wird)
pub fn pause_keys(keys: Res<ButtonInput<KeyCode>>, placing: Res<Placing>, outcome: Res<Outcome>, mut phase: ResMut<Phase>, mut help: ResMut<HelpShown>) {
    if *outcome != Outcome::Running {
        return;
    }
    let p = keys.just_pressed(KeyCode::KeyP);
    let esc = keys.just_pressed(KeyCode::Escape);
    match *phase {
        Phase::Playing if p || (esc && placing.kind.is_none()) => *phase = Phase::Paused,
        Phase::Paused if p || esc => {
            help.0 = false;
            *phase = Phase::Playing;
        }
        _ => {}
    }
}

/// Im Hauptmenü kreist die Kamera langsam über der eigenen Basis (Echtzeit, das Spiel steht still)
pub fn menu_camera(
    phase: Res<Phase>,
    time: Res<Time<Real>>,
    mut rig: ResMut<crate::input::CamRig>,
    mut cam: Query<&mut Transform, With<crate::input::MainCamera>>,
) {
    // In der Pause nur die Kamera an ihren Platz setzen (falls vor dem ersten Bild pausiert wurde)
    if *phase == Phase::Playing {
        return;
    }
    let Ok(mut t) = cam.single_mut() else { return };
    if matches!(*phase, Phase::Menu | Phase::Briefing) {
        rig.yaw += 0.05 * time.delta_secs();
    }
    rig.focus.y = crate::terrain::height(rig.focus.x, rig.focus.z);
    *t = Transform::from_translation(rig.focus + crate::input::cam_offset(&rig)).looking_at(rig.focus, Vec3::Y);
}

fn clock(secs: f32) -> String {
    let s = secs as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Ebenen ein- und ausblenden, Spielzeit anhalten, Endbildschirm füllen
pub fn update_frame(
    phase: Res<Phase>,
    outcome: Res<Outcome>,
    help: Res<HelpShown>,
    stats: Res<Stats>,
    cfg: Res<MatchConfig>,
    mut time: ResMut<Time<Virtual>>,
    mut panels: ParamSet<(
        Query<&mut Visibility, With<MenuPanel>>,
        Query<&mut Visibility, With<PausePanel>>,
        Query<&mut Visibility, With<EndPanel>>,
        Query<&mut Visibility, With<HelpPanel>>,
        Query<&mut Visibility, With<CampaignPanel>>,
        Query<&mut Visibility, With<BriefingPanel>>,
    )>,
    mut title: Query<&mut Text, (With<EndTitle>, Without<EndColumn>, Without<EndStory>)>,
    mut cols: Query<(&EndColumn, &mut Text), (Without<EndTitle>, Without<EndStory>)>,
    mut story: Query<&mut Text, (With<EndStory>, Without<EndTitle>, Without<EndColumn>)>,
    mut next: Query<&mut Node, With<NextButton>>,
    campaign: Res<CampaignShown>,
    mut shown_end: Local<bool>,
) {
    // Nach Spielende läuft die Zeit weiter, damit Explosionen und Einsturz ausklingen
    let run = *phase == Phase::Playing;
    if run && time.is_paused() {
        time.unpause();
    } else if !run && !time.is_paused() {
        time.pause();
    }
    let ended = *outcome != Outcome::Running;
    let show = |on: bool| if on { Visibility::Inherited } else { Visibility::Hidden };
    let set = |v: &mut Visibility, on: bool| {
        let want = show(on);
        if *v != want {
            *v = want;
        }
    };
    for mut v in &mut panels.p0() {
        set(&mut v, *phase == Phase::Menu);
    }
    for mut v in &mut panels.p1() {
        set(&mut v, *phase == Phase::Paused && !ended);
    }
    for mut v in &mut panels.p2() {
        set(&mut v, ended);
    }
    for mut v in &mut panels.p3() {
        set(&mut v, help.0 && *phase != Phase::Playing);
    }
    for mut v in &mut panels.p4() {
        set(&mut v, campaign.0 && *phase == Phase::Menu);
    }
    for mut v in &mut panels.p5() {
        set(&mut v, *phase == Phase::Briefing);
    }

    if ended && !*shown_end {
        *shown_end = true;
        let mission = crate::campaign::current(&cfg);
        let won = *outcome == Outcome::Won;
        let last = cfg.mission as usize == crate::campaign::MISSIONS.len();
        if let Ok(mut t) = title.single_mut() {
            **t = match (mission, won) {
                (None, true) => "SIEG",
                (None, false) => "NIEDERLAGE",
                (Some(_), true) if last => "KAMPAGNE ABGESCHLOSSEN",
                (Some(_), true) => "MISSION ERFUELLT",
                (Some(_), false) => "MISSION GESCHEITERT",
            }
            .into();
        }
        if let Ok(mut t) = story.single_mut() {
            **t = match (mission, won) {
                (Some(m), true) => m.outro.into(),
                (Some(_), false) => "Das HQ ist gefallen. VAAR hat die Kolonie zurueckerobert.".into(),
                (None, _) => String::new(),
            };
        }
        if let Ok(mut n) = next.single_mut() {
            n.display = if mission.is_some() && won && !last { Display::Flex } else { Display::None };
        }
        let rows: [(&str, [u32; 2]); 6] = [
            ("Einheiten gebaut", stats.units_built),
            ("Einheiten verloren", stats.units_lost),
            ("Gebaeude errichtet", stats.structures_built),
            ("Gebaeude verloren", stats.structures_lost),
            ("Ferrit gesammelt", [stats.gathered[0][0], stats.gathered[1][0]]),
            ("Plasma gesammelt", [stats.gathered[0][1], stats.gathered[1][1]]),
        ];
        for (col, mut t) in &mut cols {
            let mut lines = vec![match col.0 {
                0 => format!("{} | {}", cfg.difficulty.name(), clock(stats.time)),
                1 => "Du".into(),
                _ => "Gegner".into(),
            }];
            for (name, v) in rows {
                lines.push(match col.0 {
                    0 => name.into(),
                    1 => v[0].to_string(),
                    _ => v[1].to_string(),
                });
            }
            **t = lines.join("\n");
        }
    }
}

/// Ladehinweis der Webseite entfernen, sobald das erste Bild steht
pub fn hide_loading(mut frames: Local<u32>) {
    *frames += 1;
    if *frames != 3 {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    if let Some(doc) = web_sys::window().and_then(|w| w.document())
        && let Some(el) = doc.get_element_by_id("loading")
    {
        let _ = el.set_attribute("style", "display:none");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrips_through_the_address() {
        let cfg = MatchConfig { difficulty: Difficulty::Schwer, mission: 2, autostart: true };
        let q = cfg.query();
        assert_eq!(q, "?d=schwer&m=2&auto=1");
        let back = MatchConfig::parse(q.trim_start_matches('?').split('&'));
        assert_eq!((back.difficulty, back.mission, back.autostart), (Difficulty::Schwer, 2, true));
        // Programmargumente (nativ) und Unsinn
        let native = MatchConfig::parse(["--d=leicht", "--auto=1", "--x=7"].into_iter());
        assert_eq!((native.difficulty, native.mission, native.autostart), (Difficulty::Leicht, 0, true));
        let empty = MatchConfig::parse("".split('&'));
        assert_eq!((empty.difficulty, empty.autostart), (Difficulty::Normal, false));
    }

    #[test]
    fn stats_skip_start_units_and_count_losses() {
        let mut app = App::new();
        app.init_resource::<Time>().init_resource::<Stats>().add_systems(Update, track_stats);
        let unit = |team| (team, Health { hp: 100.0, max: 100.0 }, Unit { kind: UnitKind::Panzer, speed: 6.0, radius: 1.4 });
        let start = app.world_mut().spawn(unit(PLAYER)).id();
        app.update();
        app.world_mut().spawn(unit(ENEMY));
        app.world_mut().get_mut::<Health>(start).unwrap().hp = 0.0;
        app.update();
        let s = app.world().resource::<Stats>();
        assert_eq!(s.units_built, [0, 1], "Startarmee zählt nicht, neue Einheit schon");
        assert_eq!(s.units_lost, [1, 0]);
    }
}
