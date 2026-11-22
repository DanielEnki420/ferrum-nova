//! Zeitsprung: Ein Tastendruck (Z) wechselt mitten im Gefecht reihum 2026 → 2000 → 1996 → 2026.
//!
//! 2026 ist die normale Darstellung. Für die beiden alten Zeitalter rendert die Hauptkamera in ein
//! kleineres Bild, das eine zweite Kamera bildschirmfüllend zeigt (retro.wesl):
//! - 2000 (frühe 3D-Strategiespiele): 480 Zeilen, weich hochgerechnet, 16 Bit Farbtiefe (RGB565)
//!   mit feinem Dithering, sodass die typischen Farbstufen entstehen.
//! - 1996: 240 Zeilen mit harten Pixeln, fester Farbpalette, grobem Dithering und Röhren-Zeilen.
//!
//! Dazu kommen die Klänge der jeweiligen Zeit (audio.rs). Das Spiel selbst läuft unverändert
//! weiter; im Duell ist der Zeitsprung reine Anzeige auf dem eigenen Rechner.
//!
//! Die Mauskoordinaten bleiben gültig: Das kleine Bild bekommt einen Skalierungsfaktor, sodass die
//! Kamera weiter in Fensterkoordinaten rechnet.

use crate::audio::{storage_get, storage_set, PlaySfx, Sfx};
use crate::input::{depth_of_field, MainCamera};
use bevy::anti_alias::smaa::Smaa;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::image::ImageSampler;
use bevy::input::keyboard::Key;
use bevy::post_process::bloom::Bloom;
use bevy::post_process::dof::DepthOfField;
use bevy::post_process::effect_stack::Vignette;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::ui::FocusPolicy;
use bevy::window::{PrimaryWindow, WindowRef};

const STORAGE_KEY: &str = "ferrum-nova-era";

#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Era {
    #[default]
    Now,
    /// 2000: frühe 3D-Grafik mit 16 Bit Farbtiefe
    Y2000,
    /// 1996: Pixelgrafik mit Palette
    Retro,
}

impl Era {
    pub fn load() -> Self {
        Self::from_year(storage_get(STORAGE_KEY).as_deref())
    }

    fn from_year(year: Option<&str>) -> Self {
        match year {
            Some("1996") => Era::Retro,
            Some("2000") => Era::Y2000,
            _ => Era::Now,
        }
    }

    pub fn year(self) -> &'static str {
        match self {
            Era::Now => "2026",
            Era::Y2000 => "2000",
            Era::Retro => "1996",
        }
    }

    /// Nächstes Zeitalter beim Zeitsprung: 2026 → 2000 → 1996 → 2026
    pub fn next(self) -> Self {
        match self {
            Era::Now => Era::Y2000,
            Era::Y2000 => Era::Retro,
            Era::Retro => Era::Now,
        }
    }

    /// Bildzeilen der verkleinerten Darstellung (2026 rendert in voller Fenstergröße)
    fn lines(self) -> Option<f32> {
        match self {
            Era::Now => None,
            Era::Y2000 => Some(480.0),
            Era::Retro => Some(240.0),
        }
    }

    /// Darstellungsart im Shader (params.w)
    fn shader_mode(self) -> f32 {
        if self == Era::Y2000 { 1.0 } else { 0.0 }
    }
}

/// Speichersparmodus (Safari oder `?spar=1`): 2026 in einfacher statt Retina-Auflösung zeichnen
/// und hochrechnen, ohne Bloom und Schärfentiefe. Alle Bildpuffer brauchen dann nur ein Viertel des
/// Grafikspeichers; die Oberfläche bleibt scharf.
#[derive(Resource, Clone, Copy)]
pub struct LowMem(pub bool);

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct RetroMaterial {
    /// x, y: Bildgröße in Pixeln, z: Zeit (s), w: Darstellung (0 = 1996, 1 = 2000, 2 = 2026 sparsam)
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

/// Kamera, die in den alten Zeitaltern das kleine Bild und die Oberfläche ins Fenster zeichnet
#[derive(Component)]
pub struct RetroCamera;

/// Bildschirmfüllende Fläche mit dem Zeitalter-Shader (hinter der Oberfläche)
#[derive(Component)]
pub struct RetroScreen;

/// Taste Z (nach Beschriftung, also auch auf deutschen Tastaturen): Zeitsprung
pub fn era_keys(keys: Res<ButtonInput<Key>>, mut era: ResMut<Era>, mut sfx: MessageWriter<PlaySfx>) {
    let z = |c: &str| keys.just_pressed(Key::Character(c.into()));
    if z("z") || z("Z") {
        *era = era.next();
        sfx.write(PlaySfx::ui(Sfx::UiClick));
    }
}

/// Bildgröße für das Fenster: feste Zeilenzahl, Breite nach Seitenverhältnis
fn retro_size(win: &Window, lines: f32) -> UVec2 {
    let aspect = (win.width() / win.height().max(1.0)).clamp(0.5, 4.0);
    UVec2::new((lines * aspect).round() as u32, lines as u32)
}

/// Renderziel; 1996 mit harten Pixeln, 2000 weich gefiltert
fn retro_image(size: UVec2, era: Era) -> Image {
    let mut img = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
    img.sampler = if era == Era::Retro { ImageSampler::nearest() } else { ImageSampler::linear() };
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
    mut main_cam: Query<(Entity, &mut Projection), With<MainCamera>>,
    retro: Query<Entity, Or<(With<RetroCamera>, With<RetroScreen>)>>,
    low: Res<LowMem>,
    mut view: Local<Option<(Handle<Image>, Handle<RetroMaterial>, UVec2, f32, Era)>>,
    mut applied: Local<Option<Era>>,
) {
    let Ok(win) = window.single() else { return };
    let Ok((cam, mut projection)) = main_cam.single_mut() else { return };
    if applied.is_some_and(|a| a != *era) || applied.is_none() {
        storage_set(STORAGE_KEY, era.year());
    }
    // im Sparmodus läuft auch 2026 über das kleinere Bild, in einfacher Auflösung
    let lines = era.lines().or((low.0 && *era == Era::Now).then(|| win.height().round().max(1.0)));
    let mode = if *era == Era::Now { 2.0 } else { era.shader_mode() };
    let Some(lines) = lines else {
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
            // Bevy berechnet Zielgröße und Skalierung der Kamera nur bei Fenster- oder
            // Bildereignissen neu, nicht beim Wechsel des Renderziels. Ohne diesen Anstoß bliebe
            // die Kamera auf der Größe des kleinen Bildes (unscharf, Mauskoordinaten verschoben).
            projection.set_changed();
            *view = None;
        }
        *applied = Some(*era);
        return;
    };
    let size = retro_size(win, lines);
    let scale = size.y as f32 / win.height().max(1.0);
    // Anzeige-Kamera und Fläche fehlen nur, wenn vorher 2026 in voller Auflösung lief
    let fresh = retro.is_empty();
    if fresh {
        commands.entity(cam).remove::<(Smaa, DepthOfField, Vignette)>();
        if low.0 {
            commands.entity(cam).remove::<Bloom>();
        }
    }
    // Neues Bild, wenn sich Zeitalter, Fenstergröße oder Skalierung geändert hat
    if view.as_ref().is_none_or(|v| v.4 != *era || v.2 != size || (v.3 - scale).abs() > 1e-4) {
        let image = images.add(retro_image(size, *era));
        let params = Vec4::new(size.x as f32, size.y as f32, 0.0, mode);
        let mat = match view.as_ref() {
            Some((_, m, ..)) => {
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
        *view = Some((image, mat, size, scale, *era));
    }
    let Some((_, mat, ..)) = view.as_ref() else { return };
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
    *applied = Some(*era);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retro_size_follows_the_window_shape() {
        let mut w = Window::default();
        w.resolution.set(1600.0, 900.0);
        assert_eq!(retro_size(&w, 240.0), UVec2::new(427, 240));
        assert_eq!(retro_size(&w, 480.0), UVec2::new(853, 480));
        w.resolution.set(800.0, 800.0);
        assert_eq!(retro_size(&w, 240.0), UVec2::new(240, 240));
        assert_eq!(retro_size(&w, 480.0), UVec2::new(480, 480));
        w.resolution.set(1920.0, 1080.0);
        assert_eq!(retro_size(&w, Era::Y2000.lines().unwrap()), UVec2::new(853, 480));
        assert_eq!(Era::Now.lines(), None);
    }

    #[test]
    fn time_jump_cycles_through_three_eras() {
        assert_eq!(Era::Now.next(), Era::Y2000);
        assert_eq!(Era::Y2000.next(), Era::Retro);
        assert_eq!(Era::Retro.next(), Era::Now);
        for e in [Era::Now, Era::Y2000, Era::Retro] {
            assert_eq!(Era::from_year(Some(e.year())), e);
        }
        // alte und fehlende Werte bleiben gültig
        assert_eq!(Era::from_year(Some("1996")), Era::Retro);
        assert_eq!(Era::from_year(Some("2026")), Era::Now);
        assert_eq!(Era::from_year(None), Era::Now);
        assert_eq!(Era::from_year(Some("unsinn")), Era::Now);
    }
}
