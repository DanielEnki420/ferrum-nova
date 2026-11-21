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

const PANEL: Color = Color::srgba(0.05, 0.07, 0.10, 0.88);
const BTN: Color = Color::srgb(0.14, 0.20, 0.28);
const BTN_HOVER: Color = Color::srgb(0.20, 0.30, 0.42);
const BTN_PRESS: Color = Color::srgb(0.30, 0.55, 0.45);

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
    let text = format!("[{key}] {name}\n{} F / {} P", c[0], c[1]);
    let border = match item {
        BuildItem::Unit(_) => Color::srgb(0.35, 0.5, 0.65),
        BuildItem::Structure(_) => Color::srgb(0.65, 0.55, 0.3),
    };
    (
        BuildButton(item),
        RelativeCursorPosition::default(),
        Node {
            width: px(118),
            flex_shrink: 0.0,
            height: px(56),
            padding: UiRect::all(px(6)),
            margin: UiRect::right(px(6)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(6)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor::all(border),
        BackgroundColor(BTN),
        children![label(text, 13.0)],
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
            label("WASD Kamera | Q/E drehen | Rad Zoom | Leertaste Basis", 13.0),
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
            build_button(BuildItem::Unit(UnitKind::Sammler), "1"),
            build_button(BuildItem::Unit(UnitKind::Panzer), "2"),
            build_button(BuildItem::Unit(UnitKind::Werfer), "3"),
            build_button(BuildItem::Structure(BuildingKind::Geschuetzturm), "4"),
            build_button(BuildItem::Structure(BuildingKind::Raffinerie), "5"),
            build_button(BuildItem::Structure(BuildingKind::Fabrik), "6"),
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    margin: UiRect::left(px(12)),
                    row_gap: px(6),
                    ..default()
                },
                children![(label("", 14.0), QueueText), (label("", 14.0), SelectionText)],
            ),
        ],
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
        *bg = if active || (over && (pressed_now || mouse.pressed(MouseButton::Left))) {
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
    for (k, kind) in [
        (KeyCode::Digit1, UnitKind::Sammler),
        (KeyCode::Digit2, UnitKind::Panzer),
        (KeyCode::Digit3, UnitKind::Werfer),
    ] {
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
    placing: Res<Placing>,
    mut texts: ParamSet<(
        Query<&mut Text, With<StockText>>,
        Query<&mut Text, With<QueueText>>,
        Query<&mut Text, With<SelectionText>>,
        Query<&mut Text, With<OutcomeText>>,
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
    let mut counts = [0; 3];
    for (team, u, sel) in &units {
        if sel && *team == PLAYER {
            counts[u.kind as usize] += 1;
        }
    }
    let sel_text = if let Some(k) = placing.kind {
        format!("{} platzieren: Linksklick | Shift: mehrere | Rechtsklick/Esc: abbrechen", k.name())
    } else if counts.iter().sum::<i32>() == 0 {
        "Links: auswaehlen | F: alle Kampfeinheiten | Rechts: bewegen/angreifen/abbauen | Strg+Rechts: Angriffsmarsch".to_string()
    } else {
        format!(
            "Auswahl: {} Sammler, {} Panzer, {} Werfer",
            counts[0], counts[1], counts[2]
        )
    };
    if let Ok(mut t) = texts.p2().single_mut() {
        **t = sel_text;
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
