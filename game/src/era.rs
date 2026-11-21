//! Zeitsprung: Ein Tastendruck (Z) wechselt mitten im Gefecht zwischen 2026 und 1996.
//!
//! 2026 ist die normale Darstellung. Für 1996 rendert die Hauptkamera in ein kleines Bild
//! (240 Zeilen). Eine zweite Kamera zeigt dieses Bild bildschirmfüllend mit harten Pixeln, einer
//! festen Farbpalette mit Rasterpunkten (Dithering) und Röhren-Zeilen (retro.wesl). Dazu kommen die
//! Klänge im Stil einer alten Soundkarte (audio.rs). Das Spiel selbst läuft unverändert weiter.
//!
//! Die Mauskoordinaten bleiben gültig: Das kleine Bild bekommt einen Skalierungsfaktor, sodass die
//! Kamera weiter in Fensterkoordinaten rechnet.

use crate::audio::{storage_get, storage_set, PlaySfx, Sfx};
use crate::input::{depth_of_field, MainCamera};
use bevy::anti_alias::smaa::Smaa;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::image::ImageSampler;
use bevy::input::keyboard::Key;
use bevy::post_process::dof::DepthOfField;
use bevy::post_process::effect_stack::Vignette;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::ui::FocusPolicy;
use bevy::window::{PrimaryWindow, WindowRef};

/// Bildzeilen im 1996-Modus
const RETRO_LINES: f32 = 240.0;
const STORAGE_KEY: &str = "ferrum-nova-era";

#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Era {
    #[default]
    Now,
    Retro,
}

impl Era {
    pub fn load() -> Self {
        if storage_get(STORAGE_KEY).as_deref() == Some("1996") { Era::Retro } else { Era::Now }
    }

    pub fn year(self) -> &'static str {
        match self {
            Era::Now => "2026",
            Era::Retro => "1996",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Era::Now => Era::Retro,
            Era::Retro => Era::Now,
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct RetroMaterial {
    /// x, y: Bildgröße in Pixeln, z: Zeit (s), w: frei
    #[uniform(0)]
    pub params: Vec4,
    #[texture(1)]
    #[sampler(2)]
    pub screen: Handle<Image>,
}

impl UiMaterial for RetroMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/retro.wesl".into()
    }
}

/// Kamera, die im 1996-Modus das kleine Bild und die Oberfläche ins Fenster zeichnet
#[derive(Component)]
pub struct RetroCamera;

/// Bildschirmfüllende Fläche mit dem Retro-Shader (hinter der Oberfläche)
#[derive(Component)]
pub struct RetroScreen;

/// Taste Z (nach Beschriftung, also auch auf deutschen Tastaturen): Zeitsprung
pub fn era_keys(keys: Res<ButtonInput<Key>>, mut era: ResMut<Era>, mut sfx: MessageWriter<PlaySfx>) {
    let z = |c: &str| keys.just_pressed(Key::Character(c.into()));
    if z("z") || z("Z") {
        *era = era.toggled();
        sfx.write(PlaySfx::ui(Sfx::UiClick));
    }
}

/// Bildgröße für das Fenster: feste Zeilenzahl, Breite nach Seitenverhältnis
fn retro_size(win: &Window) -> UVec2 {
    let aspect = (win.width() / win.height().max(1.0)).clamp(0.5, 4.0);
    UVec2::new((RETRO_LINES * aspect).round() as u32, RETRO_LINES as u32)
}

fn retro_image(size: UVec2) -> Image {
    let mut img = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
    img.sampler = ImageSampler::nearest();
    img
}

/// Zeitalter umsetzen: Renderziel, Nachbearbeitung, Anzeige-Kamera; Bildgröße dem Fenster nachführen
pub fn apply_era(
    mut commands: Commands,
    era: Res<Era>,
    time: Res<Time<Real>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<RetroMaterial>>,
    main_cam: Query<Entity, With<MainCamera>>,
    retro: Query<Entity, Or<(With<RetroCamera>, With<RetroScreen>)>>,
    mut view: Local<Option<(Handle<Image>, Handle<RetroMaterial>, UVec2, f32)>>,
    mut applied: Local<Option<Era>>,
) {
    let Ok(win) = window.single() else { return };
    let Ok(cam) = main_cam.single() else { return };
    if applied.is_some_and(|a| a != *era) || applied.is_none() {
        storage_set(STORAGE_KEY, era.year());
    }
    match *era {
        Era::Now => {
            if *applied != Some(Era::Now) {
                for e in &retro {
                    commands.entity(e).despawn();
                }
                commands.entity(cam).insert((
                    RenderTarget::Window(WindowRef::Primary),
                    Smaa::default(),
                    depth_of_field(),
                    Vignette { intensity: 0.42, radius: 0.85, smoothness: 3.5, ..default() },
                ));
                *view = None;
            }
        }
        Era::Retro => {
            let size = retro_size(win);
            let scale = size.y as f32 / win.height().max(1.0);
            let fresh = *applied != Some(Era::Retro);
            if fresh {
                commands.entity(cam).remove::<(Smaa, DepthOfField, Vignette)>();
            }
            // Neues Bild, wenn sich Fenstergröße oder Skalierung geändert hat
            if view.as_ref().is_none_or(|v| v.2 != size || (v.3 - scale).abs() > 1e-4) {
                let image = images.add(retro_image(size));
                let params = Vec4::new(size.x as f32, size.y as f32, 0.0, 0.0);
                let mat = match view.as_ref() {
                    Some((_, m, _, _)) => {
                        if let Some(mut m2) = mats.get_mut(m) {
                            m2.screen = image.clone();
                            m2.params = params;
                        }
                        m.clone()
                    }
                    None => mats.add(RetroMaterial { params, screen: image.clone() }),
                };
                commands
                    .entity(cam)
                    .insert(RenderTarget::Image(ImageRenderTarget { handle: image.clone(), scale_factor: scale }));
                *view = Some((image, mat, size, scale));
            }
            let Some((_, mat, _, _)) = view.as_ref() else { return };
            if fresh {
                commands.spawn((
                    RetroCamera,
                    Camera2d,
                    Camera { order: 1, ..default() },
                    Msaa::Off,
                    IsDefaultUiCamera,
                ));
                commands.spawn((
                    RetroScreen,
                    MaterialNode(mat.clone()),
                    Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
                    FocusPolicy::Pass,
                    GlobalZIndex(i32::MIN),
                ));
            }
            // Zeit für das leichte Flimmern der Röhre
            if let Some(mut m) = mats.get_mut(mat) {
                m.params.z = time.elapsed_secs() % 1000.0;
            }
        }
    }
    *applied = Some(*era);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retro_size_follows_the_window_shape() {
        let mut w = Window::default();
        w.resolution.set(1600.0, 900.0);
        assert_eq!(retro_size(&w), UVec2::new(427, 240));
        w.resolution.set(800.0, 800.0);
        assert_eq!(retro_size(&w), UVec2::new(240, 240));
        assert_eq!(Era::Now.toggled(), Era::Retro);
        assert_eq!(Era::Retro.year(), "1996");
    }
}
