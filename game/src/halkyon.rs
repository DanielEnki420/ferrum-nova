//! Halkyon-Finsternis: Der Gasriese Halkyon schiebt sich in regelmäßigen Abständen vor die Sonne.
//!
//! Dann wird es auf Nova für eine Minute Nacht: Sonne und Himmelslicht verlöschen, die Gebäude
//! schalten ihre Lichter an (blau beim Spieler, rot bei VAAR), die eigene Sicht schrumpft (fog.rs) und
//! VAAR nutzt die Dunkelheit für einen Angriff. Oben rechts zählt eine Anzeige bis zur nächsten
//! Finsternis herunter. Die Uhr läuft nur im Spiel (virtuelle Zeit), nicht im Menü oder in der Pause.

use crate::ai::AiState;
use crate::audio::{PlaySfx, Sfx};
use crate::components::*;
use crate::hud::TOP_BAR_H;
use crate::input::{MainCamera, HAZE};
use bevy::light::EnvironmentMapLight;
use bevy::prelude::*;

/// Erste Finsternis nach dieser Spielzeit (s)
const FIRST: f32 = 200.0;
/// Abstand zwischen zwei Finsternissen (s)
const PERIOD: f32 = 300.0;
/// Dämmerung, volle Nacht, Morgen (s)
const DUSK: f32 = 20.0;
const DARK: f32 = 60.0;
const DAWN: f32 = 20.0;

/// Wie stark die Sicht des Spielers in voller Nacht schrumpft (Einheiten / Gebäude)
pub const UNIT_SIGHT_LOSS: f32 = 0.4;
pub const BUILDING_SIGHT_LOSS: f32 = 0.25;

#[derive(Resource, Default)]
pub struct Eclipse {
    /// Spielzeit (s)
    pub clock: f32,
    /// 0 = Tag, 1 = volle Finsternis
    pub shade: f32,
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Position im Zyklus (s seit Beginn der aktuellen Finsternis), vor der ersten: negativ
fn phase(t: f32) -> f32 {
    if t < FIRST { t - FIRST } else { (t - FIRST) % PERIOD }
}

/// Dunkelheit zur Spielzeit `t`
pub fn shade_at(t: f32) -> f32 {
    let p = phase(t);
    if p < 0.0 {
        0.0
    } else if p < DUSK {
        smooth(p / DUSK)
    } else if p < DUSK + DARK {
        1.0
    } else {
        1.0 - smooth((p - DUSK - DARK) / DAWN)
    }
}

/// Sekunden bis zur nächsten Dämmerung (0, solange eine Finsternis läuft)
pub fn until_next(t: f32) -> f32 {
    let p = phase(t);
    if p < 0.0 {
        -p
    } else if p < DUSK + DARK + DAWN {
        0.0
    } else {
        PERIOD - p
    }
}

/// Licht einer Gebäudeleuchte (Kind des Gebäudes)
#[derive(Component)]
pub struct EclipseLight(f32);

/// Gebäude hat seine Leuchte schon
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Lit;

#[derive(Component)]
pub struct EclipseText;

pub fn setup_eclipse_hud(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(TOP_BAR_H + 6.0),
            right: px(10),
            padding: UiRect::axes(px(8), px(4)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.07, 0.10, 0.7)),
        children![(
            Text::new(""),
            TextFont { font_size: FontSize::Px(13.0), ..default() },
            TextColor(Color::srgb(0.75, 0.82, 0.9)),
            EclipseText,
        )],
    ));
}

fn mix(a: Color, b: Color, k: f32) -> Color {
    Color::LinearRgba(a.to_linear().mix(&b.to_linear(), k))
}

const NIGHT: Color = Color::srgb(0.07, 0.08, 0.13);

/// Uhr der Finsternis, im Simulationstakt (im Duell auf beiden Rechnern gleich). Zu Beginn der
/// Dämmerung zieht VAAR im Einzelspieler den nächsten Angriff vor.
pub fn tick_clock(
    time: Res<Time>,
    mut eclipse: ResMut<Eclipse>,
    mut ai: ResMut<AiState>,
    mut sfx: MessageWriter<PlaySfx>,
    rules: Option<Res<crate::sim::SimRules>>,
) {
    let before = eclipse.clock;
    eclipse.clock += time.delta_secs();
    let t = eclipse.clock;
    eclipse.shade = shade_at(t);
    if until_next(before) > 0.0 && until_next(t) == 0.0 {
        sfx.write(PlaySfx::ui(Sfx::Alert));
        if !rules.is_some_and(|r| r.duel) && ai.pace.eclipse_rush && ai.wave.is_none() && ai.next_attack - ai.elapsed < 120.0 {
            ai.next_attack = ai.elapsed + DUSK * 0.6;
        }
    }
}

/// Licht und Anzeige (Darstellung, jedes Bild)
pub fn update_eclipse(
    eclipse: Res<Eclipse>,
    mut sun: Query<&mut DirectionalLight>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut cam: Query<(&mut EnvironmentMapLight, &mut DistanceFog), With<MainCamera>>,
    mut text: Query<(&mut Text, &mut TextColor), With<EclipseText>>,
    mut shown: Local<Option<f32>>,
) {
    let t = eclipse.clock;
    let s = eclipse.shade;

    if shown.is_none_or(|v| (v - s).abs() > 1e-3) {
        *shown = Some(s);
        for mut l in &mut sun {
            l.illuminance = 11_500.0 + (2_200.0 - 11_500.0) * s;
            l.color = mix(Color::srgb(1.0, 0.89, 0.74), Color::srgb(0.55, 0.65, 1.0), s);
        }
        ambient.brightness = 90.0 - 40.0 * s;
        ambient.color = mix(Color::srgb(0.62, 0.70, 0.85), Color::srgb(0.35, 0.42, 0.75), s);
        clear.0 = mix(HAZE, NIGHT, s);
        for (mut env, mut fog) in &mut cam {
            env.intensity = 700.0 - 450.0 * s;
            fog.color = mix(HAZE, NIGHT, s);
            fog.directional_light_color = mix(Color::srgba(1.0, 0.85, 0.65, 0.35), Color::srgba(0.4, 0.5, 0.9, 0.1), s);
        }
    }

    let Ok((mut txt, mut color)) = text.single_mut() else { return };
    let wait = until_next(t);
    let (msg, c) = if wait > 0.0 {
        let w = wait.ceil() as u32;
        (format!("Halkyon-Finsternis in {}:{:02}", w / 60, w % 60), Color::srgb(0.75, 0.82, 0.9))
    } else {
        let left = (DUSK + DARK + DAWN - phase(t)).max(0.0).ceil() as u32;
        (format!("FINSTERNIS {}:{:02}  |  Sicht verringert", left / 60, left % 60), Color::srgb(1.0, 0.75, 0.35))
    };
    if **txt != msg {
        **txt = msg;
    }
    if color.0 != c {
        color.0 = c;
    }
}

/// Fertige Gebäude bekommen eine Leuchte, die nur in der Finsternis brennt
pub fn eclipse_lights(
    mut commands: Commands,
    eclipse: Res<Eclipse>,
    new: Query<(Entity, &Team, &Building), (Without<Lit>, Without<Construction>, Without<Wall>)>,
    mut lights: Query<(&EclipseLight, &mut PointLight, &mut Visibility)>,
) {
    for (e, team, b) in &new {
        let color = if *team == PLAYER { Color::srgb(0.6, 0.8, 1.0) } else { Color::srgb(1.0, 0.35, 0.22) };
        let power = 120_000.0 + b.radius * 60_000.0;
        commands.entity(e).insert(Lit).with_child((
            EclipseLight(power),
            PointLight { color, intensity: 0.0, range: 10.0 + b.radius * 3.0, shadow_maps_enabled: false, ..default() },
            Transform::from_xyz(0.0, 4.0 + b.radius * 0.5, 0.0),
            Visibility::Hidden,
        ));
    }
    let s = eclipse.shade;
    for (l, mut p, mut vis) in &mut lights {
        let want = if s > 0.02 { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        let i = l.0 * s;
        if (p.intensity - i).abs() > 1.0 {
            p.intensity = i;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eclipse_cycle_has_dusk_night_and_dawn() {
        assert_eq!(shade_at(0.0), 0.0);
        assert_eq!(until_next(0.0), FIRST);
        assert!(shade_at(FIRST + DUSK * 0.5) > 0.3 && shade_at(FIRST + DUSK * 0.5) < 0.7);
        assert_eq!(shade_at(FIRST + DUSK + 1.0), 1.0);
        assert_eq!(until_next(FIRST + DUSK + 1.0), 0.0);
        assert_eq!(shade_at(FIRST + DUSK + DARK + DAWN + 1.0), 0.0);
        assert!((until_next(FIRST + DUSK + DARK + DAWN) - (PERIOD - DUSK - DARK - DAWN)).abs() < 1e-3);
        // zweite Finsternis wie die erste
        assert_eq!(shade_at(FIRST + PERIOD + DUSK + 5.0), 1.0);
    }
}
