use crate::components::*;
use crate::input::SelectionBox;
use crate::construction::Placing;
use crate::units::{production_rate, BuildRequest};
use bevy::prelude::*;
use bevy::input::ButtonState;
use bevy::input::mouse::MouseButtonInput;
use bevy::ui::RelativeCursorPosition;

/// Bau-Knopf im HUD. Ausgelöst direkt beim Drücken, damit auch sehr kurze Klicks
/// (z. B. Trackpad-Tippen) zuverlässig ankommen.
#[derive(Component)]
pub struct BuildButton(pub BuildItem);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BuildItem {
    Unit(UnitKind),
    Structure(BuildingKind),
}

pub const TOP_BAR_H: f32 = 36.0;
pub const BOTTOM_PANEL_H: f32 = 96.0;

#[derive(Component)]
pub struct StockText;
#[derive(Component)]
pub struct QueueText;
#[derive(Component)]
pub struct SelectionText;
#[derive(Component)]
pub struct OutcomeText;
#[derive(Component)]
pub struct AlertText;

/// Leiste mit den ausgewählten Einheiten (über der unteren Leiste)
#[derive(Component)]
pub struct SelectionPanel;

/// Karte einer ausgewählten Einheit; Klick wählt nur sie, Shift+Klick nimmt sie heraus
#[derive(Component)]
pub struct SelCard(pub Entity);

/// Lebensbalken einer Karte
#[derive(Component)]
pub struct SelCardBar(pub Entity);

/// Tasten für die Bau-Knöpfe: Einheiten in `UnitKind::ALL`-Reihenfolge, dann Gebäude
pub const UNIT_KEYS: [(KeyCode, &str); 6] = [
    (KeyCode::KeyX, "X"),
    (KeyCode::KeyC, "C"),
    (KeyCode::KeyV, "V"),
    (KeyCode::KeyB, "B"),
    (KeyCode::KeyN, "N"),
    (KeyCode::KeyM, "M"),
];
const MAX_CARDS: usize = 24;

const PANEL: Color = Color::srgba(0.05, 0.07, 0.10, 0.88);
const BTN: Color = Color::srgb(0.14, 0.20, 0.28);
const BTN_HOVER: Color = Color::srgb(0.20, 0.30, 0.42);
const BTN_PRESS: Color = Color::srgb(0.30, 0.55, 0.45);
/// Gesperrt: braucht erst eine Fabrik
const BTN_LOCKED: Color = Color::srgb(0.10, 0.11, 0.13);

fn label(text: impl Into<String>, size: f32) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(Color::srgb(0.88, 0.92, 0.96)),
    )
}

fn build_button(item: BuildItem, key: &str) -> impl Bundle {
    let (name, c) = match item {
        BuildItem::Unit(k) => (k.short_name(), k.cost()),
        BuildItem::Structure(k) => (k.short_name(), k.cost()),
    };
    let text = format!("[{key}] {name}\n{}F / {}P", c[0], c[1]);
    let border = match item {
        BuildItem::Unit(_) => Color::srgb(0.35, 0.5, 0.65),
        BuildItem::Structure(_) => Color::srgb(0.65, 0.55, 0.3),
    };
    (
        BuildButton(item),
        RelativeCursorPosition::default(),
        Node {
            width: px(96),
            flex_shrink: 0.0,
            height: px(56),
            padding: UiRect::all(px(6)),
            margin: UiRect::right(px(5)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(6)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor::all(border),
        BackgroundColor(BTN),
        children![label(text, 12.0)],
    )
}

pub fn setup_hud(mut commands: Commands) {
    // Obere Leiste
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(0),
            left: px(0),
            width: percent(100),
            height: px(TOP_BAR_H),
            padding: UiRect::horizontal(px(14)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        },
        BackgroundColor(PANEL),
        children![
            label_col("FERRUM NOVA", 16.0, Color::srgb(0.55, 0.85, 1.0)),
            (label("", 16.0), StockText),
            label("WASD/Mitteltaste Kamera | Q/E drehen | Rad Zoom | Leertaste letzter Angriff | Pos1 Basis", 13.0),
        ],
    ));

    // Untere Leiste
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: px(0),
            left: px(0),
            width: percent(100),
            height: px(BOTTOM_PANEL_H),
            padding: UiRect::all(px(12)),
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(PANEL),
        children![
            build_button(BuildItem::Unit(UnitKind::ALL[0]), UNIT_KEYS[0].1),
            build_button(BuildItem::Unit(UnitKind::ALL[1]), UNIT_KEYS[1].1),
            build_button(BuildItem::Unit(UnitKind::ALL[2]), UNIT_KEYS[2].1),
            build_button(BuildItem::Unit(UnitKind::ALL[3]), UNIT_KEYS[3].1),
            build_button(BuildItem::Unit(UnitKind::ALL[4]), UNIT_KEYS[4].1),
            build_button(BuildItem::Unit(UnitKind::ALL[5]), UNIT_KEYS[5].1),
            build_button(BuildItem::Structure(BuildingKind::Geschuetzturm), "G"),
            build_button(BuildItem::Structure(BuildingKind::Raffinerie), "H"),
            build_button(BuildItem::Structure(BuildingKind::Fabrik), "J"),
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    margin: UiRect::left(px(12)),
                    row_gap: px(6),
                    ..default()
                },
                children![(label("", 14.0), QueueText)],
            ),
        ],
    ));

    // Ausgewählte Einheiten
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(8),
            bottom: px(BOTTOM_PANEL_H + 44.0),
            // feste Breite: 8 Karten je Reihe, weitere Reihen wachsen nach oben
            width: px(8.0 * 68.0),
            flex_wrap: FlexWrap::WrapReverse,
            column_gap: px(4),
            row_gap: px(4),
            display: Display::None,
            ..default()
        },
        SelectionPanel,
    ));

    // Bedienhinweise passend zur Auswahl
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(8),
            bottom: px(BOTTOM_PANEL_H + 4.0),
            padding: UiRect::axes(px(6), px(2)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.07, 0.10, 0.6)),
        children![(label_col("", 12.0, Color::srgb(0.75, 0.82, 0.9)), SelectionText)],
    ));

    // Angriffsmeldung
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            top: px(TOP_BAR_H + 10.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(label_col("", 18.0, Color::srgb(1.0, 0.45, 0.35)), AlertText)],
    ));

    // Auswahlbox
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            border: UiRect::all(px(1)),
            ..default()
        },
        BorderColor::all(Color::srgb(0.4, 1.0, 0.5)),
        BackgroundColor(Color::srgba(0.4, 1.0, 0.5, 0.08)),
        Visibility::Hidden,
        SelectionBox,
    ));

    // Sieg/Niederlage
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            top: percent(40),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(label("", 54.0), OutcomeText)],
    ));
}

pub fn build_buttons(
    mut clicks: MessageReader<MouseButtonInput>,
    mouse: Res<ButtonInput<MouseButton>>,
    factories: Query<(&Team, &Structure), Without<Construction>>,
    mut q: Query<(&BuildButton, &RelativeCursorPosition, &mut BackgroundColor)>,
    mut w: MessageWriter<BuildRequest>,
    mut placing: ResMut<Placing>,
) {
    let pressed_now = clicks
        .read()
        .any(|c| c.button == MouseButton::Left && c.state == ButtonState::Pressed);
    for (btn, rel, mut bg) in &mut q {
        let over = rel.cursor_over();
        if over && pressed_now {
            match btn.0 {
                BuildItem::Unit(kind) => {
                    w.write(BuildRequest { team: PLAYER, kind });
                }
                BuildItem::Structure(kind) => {
                    placing.kind = if placing.kind == Some(kind) { None } else { Some(kind) };
                }
            }
        }
        let active = matches!(btn.0, BuildItem::Structure(k) if placing.kind == Some(k));
        let locked = matches!(btn.0, BuildItem::Unit(k) if k.needs_factory() && !crate::units::has_factory(&factories, PLAYER));
        *bg = if locked {
            BTN_LOCKED
        } else if active || (over && (pressed_now || mouse.pressed(MouseButton::Left))) {
            BTN_PRESS
        } else if over {
            BTN_HOVER
        } else {
            BTN
        }
        .into();
    }
}

pub fn build_hotkeys(keys: Res<ButtonInput<KeyCode>>, mut w: MessageWriter<BuildRequest>) {
    // Strg/Cmd+Taste gehört dem Browser bzw. den Kontrollgruppen
    if crate::input::modifiers(&keys).1 {
        return;
    }
    for ((k, _), kind) in UNIT_KEYS.into_iter().zip(UnitKind::ALL) {
        if keys.just_pressed(k) {
            w.write(BuildRequest { team: PLAYER, kind });
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn update_hud(
    stock: Res<Stock>,
    outcome: Res<Outcome>,
    hqs: Query<(&Team, &Hq)>,
    units: Query<(&Team, &Unit, Has<Selected>)>,
    structures: Query<(&Team, &Structure), Without<Construction>>,
    selected_hq: Query<&Team, (With<Hq>, With<Selected>)>,
    placing: Res<Placing>,
    alerts: Res<crate::orders::Alerts>,
    mut texts: ParamSet<(
        Query<&mut Text, With<StockText>>,
        Query<&mut Text, With<QueueText>>,
        Query<&mut Text, With<SelectionText>>,
        Query<&mut Text, With<OutcomeText>>,
        Query<&mut Text, With<AlertText>>,
    )>,
) {
    let s = stock.0[PLAYER.0 as usize];
    let count = units.iter().filter(|(t, _, _)| **t == PLAYER).count();
    if let Ok(mut t) = texts.p0().single_mut() {
        **t = format!("Ferrit: {}    Plasma: {}    Einheiten: {}", s[0], s[1], count);
    }
    let queue = hqs
        .iter()
        .find(|(t, _)| **t == PLAYER)
        .map(|(_, hq)| {
            if hq.queue.is_empty() {
                "Bauschleife: leer".to_string()
            } else {
                let first = hq.queue[0];
                let pct = (hq.progress / first.build_time() * 100.0) as u32;
                let names: Vec<_> = hq.queue.iter().map(|k| k.name()).collect();
                format!("Bauschleife ({pct}%): {}", names.join(", "))
            }
        })
        .unwrap_or_default();
    let factories = structures.iter().filter(|(t, s)| **t == PLAYER && s.0 == BuildingKind::Fabrik).count();
    let queue = if factories > 0 {
        format!("{queue}   [Fabriken: {factories}, Tempo {:.0} %]", production_rate(factories) * 100.0)
    } else {
        queue
    };
    if let Ok(mut t) = texts.p1().single_mut() {
        **t = queue;
    }
    let mut counts = [0; 6];
    for (team, u, sel) in &units {
        if sel && *team == PLAYER {
            counts[u.kind.idx()] += 1;
        }
    }
    let sel_text = if let Some(k) = placing.kind {
        format!("{} platzieren: Linksklick | Shift: mehrere | Rechtsklick/Esc: abbrechen", k.name())
    } else if counts.iter().sum::<i32>() == 0 && selected_hq.iter().any(|t| *t == PLAYER) {
        "Hauptquartier: Rechtsklick setzt den Sammelpunkt (auf einem Feld: Sammler bauen dort ab)".to_string()
    } else if counts.iter().sum::<i32>() == 0 {
        "Links: auswaehlen | Doppelklick: gleicher Typ | F: alle Kampfeinheiten | 1-9: Gruppe abrufen".to_string()
    } else {
        "Rechts: bewegen/angreifen | ziehen: Frontlinie | Shift: anhaengen | Strg: Angriffsmarsch\nAlt: Patrouille | R: Stopp | T: Stellung halten | Strg+1-9: Gruppe speichern".to_string()
    };
    if let Ok(mut t) = texts.p2().single_mut() {
        **t = sel_text;
    }
    if let Ok(mut t) = texts.p4().single_mut() {
        **t = match alerts.last {
            Some(_) if alerts.since < 4.0 => "Wir werden angegriffen!  [Leertaste]".to_string(),
            _ => String::new(),
        };
    }
    if let Ok(mut t) = texts.p3().single_mut() {
        **t = match *outcome {
            Outcome::Running => String::new(),
            Outcome::Won => "SIEG - der Schwarm ist zerschlagen".into(),
            Outcome::Lost => "NIEDERLAGE - die Basis ist gefallen".into(),
        };
    }
}

fn label_col(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

/// HUD auf kleinen Fenstern (Laptop, Tablet, eingebettet) verkleinern
pub fn scale_ui(
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut ui_scale: ResMut<UiScale>,
) {
    let Ok(win) = window.single() else { return };
    let s = (win.width() / 1150.0).clamp(0.55, 1.0);
    if (ui_scale.0 - s).abs() > 0.01 {
        ui_scale.0 = s;
    }
}

/// Leistungsanzeige (F3 schaltet sie ein und aus)
#[derive(Component)]
pub struct PerfText;

pub fn setup_perf(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(TOP_BAR_H + 6.0),
            left: px(10),
            padding: UiRect::axes(px(8), px(4)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
        PerfText,
        children![label_col("", 13.0, Color::srgb(0.6, 1.0, 0.6))],
    ));
}

pub fn update_perf(
    keys: Res<ButtonInput<KeyCode>>,
    diags: Res<bevy::diagnostic::DiagnosticsStore>,
    mut panel: Query<(&mut Visibility, &Children), With<PerfText>>,
    mut texts: Query<&mut Text>,
    entities: Query<Entity>,
    time: Res<Time>,
    mut timer: Local<f32>,
) {
    use bevy::diagnostic::FrameTimeDiagnosticsPlugin as F;
    let Ok((mut vis, children)) = panel.single_mut() else { return };
    if keys.just_pressed(KeyCode::F3) {
        *vis = if *vis == Visibility::Hidden { Visibility::Inherited } else { Visibility::Hidden };
    }
    *timer -= time.delta_secs();
    if *timer > 0.0 || *vis == Visibility::Hidden {
        return;
    }
    *timer = 0.5;
    let fps = diags.get(&F::FPS).and_then(|d| d.smoothed()).unwrap_or(0.0);
    let ms = diags.get(&F::FRAME_TIME).and_then(|d| d.smoothed()).unwrap_or(0.0);
    if let Some(&c) = children.first()
        && let Ok(mut t) = texts.get_mut(c)
    {
        t.0 = format!("{fps:.0} FPS | {ms:.1} ms | {} Objekte", entities.iter().count());
    }
}

/// Karten der ausgewählten Einheiten neu aufbauen, wenn sich die Auswahl ändert; Lebensbalken laufend
pub fn update_selection_panel(
    mut commands: Commands,
    sel: Query<(Entity, &Team, &Unit, &Health), With<Selected>>,
    mut panel: Query<(Entity, &mut Node), With<SelectionPanel>>,
    mut bars: Query<(&SelCardBar, &mut Node, &mut BackgroundColor), Without<SelectionPanel>>,
    mut shown: Local<Vec<Entity>>,
) {
    let Ok((panel_e, mut panel_node)) = panel.single_mut() else { return };
    let mut list: Vec<(Entity, UnitKind)> = sel.iter().filter(|s| *s.1 == PLAYER).map(|s| (s.0, s.2.kind)).collect();
    list.sort_by_key(|(e, k)| (k.idx(), *e));
    let ids: Vec<Entity> = list.iter().map(|l| l.0).collect();
    if ids != *shown {
        *shown = ids;
        commands.entity(panel_e).despawn_children();
        panel_node.display = if list.is_empty() { Display::None } else { Display::Flex };
        for (e, kind) in list.iter().take(MAX_CARDS) {
            commands.spawn((
                ChildOf(panel_e),
                SelCard(*e),
                RelativeCursorPosition::default(),
                Node {
                    width: px(64),
                    height: px(34),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceBetween,
                    padding: UiRect::all(px(3)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(4)),
                    ..default()
                },
                BorderColor::all(Color::srgb(0.35, 0.5, 0.65)),
                BackgroundColor(BTN),
                children![
                    label(kind.short_name(), 11.0),
                    (
                        Node { width: percent(100), height: px(4), ..default() },
                        BackgroundColor(Color::srgb(0.12, 0.12, 0.12)),
                        children![(SelCardBar(*e), Node { width: percent(100), height: percent(100), ..default() }, BackgroundColor(Color::srgb(0.3, 1.0, 0.4)))],
                    )
                ],
            ));
        }
        if list.len() > MAX_CARDS {
            commands.spawn((ChildOf(panel_e), label(format!("+{}", list.len() - MAX_CARDS), 14.0)));
        }
    }
    for (bar, mut node, mut bg) in &mut bars {
        let Ok((_, _, _, h)) = sel.get(bar.0) else { continue };
        let frac = (h.hp / h.max).clamp(0.0, 1.0);
        node.width = percent(frac * 100.0);
        let col = if frac > 0.6 {
            Color::srgb(0.3, 1.0, 0.4)
        } else if frac > 0.3 {
            Color::srgb(1.0, 0.8, 0.2)
        } else {
            Color::srgb(1.0, 0.3, 0.25)
        };
        *bg = col.into();
    }
}

/// Klick auf eine Karte: nur diese Einheit auswählen; Shift+Klick nimmt sie heraus
pub fn selection_cards(
    mut commands: Commands,
    mut clicks: MessageReader<MouseButtonInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cards: Query<(&SelCard, &RelativeCursorPosition, &mut BackgroundColor)>,
    sel: Query<Entity, With<Selected>>,
) {
    let pressed = clicks.read().any(|c| c.button == MouseButton::Left && c.state == ButtonState::Pressed);
    let shift = crate::input::modifiers(&keys).0;
    for (card, rel, mut bg) in &mut cards {
        let over = rel.cursor_over();
        *bg = if over { BTN_HOVER } else { BTN }.into();
        if !(over && pressed) {
            continue;
        }
        if shift {
            commands.entity(card.0).remove::<Selected>();
        } else {
            for e in &sel {
                if e != card.0 {
                    commands.entity(e).remove::<Selected>();
                }
            }
        }
    }
}
