//! Ton: Waffen, Treffer, Explosionen, Meldungen und Musik.
//!
//! Alle Klänge sind synthetisch erzeugt (tools/make_sounds.py) und liegen als Ogg Vorbis in
//! assets/sounds. Lautstärke nach Entfernung zum Blickpunkt der Kamera; damit große Gefechte
//! nicht übersteuern, gibt es Mindestabstände und eine Obergrenze gleichzeitiger Stimmen.
//! Im Browser startet der Ton erst nach dem ersten Klick (Autoplay-Regel, siehe index.html).

use crate::components::*;
use crate::era::Era;
use crate::fx::FxEvent;
use crate::input::CamRig;
use bevy::audio::{AudioSink, AudioSinkPlayback, PlaybackMode, Volume};
use bevy::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sfx {
    ShotLight,
    ShotMedium,
    ShotHeavy,
    ShotArtillery,
    Rocket,
    ImpactGround,
    ImpactArmor,
    ExplosionSmall,
    ExplosionBig,
    BuildPlace,
    BuildDone,
    UnitReady,
    UiClick,
    Ack,
    Select,
    Denied,
    Alert,
    Victory,
    Defeat,
}

impl Sfx {
    const ALL: [Sfx; 19] = [
        Sfx::ShotLight,
        Sfx::ShotMedium,
        Sfx::ShotHeavy,
        Sfx::ShotArtillery,
        Sfx::Rocket,
        Sfx::ImpactGround,
        Sfx::ImpactArmor,
        Sfx::ExplosionSmall,
        Sfx::ExplosionBig,
        Sfx::BuildPlace,
        Sfx::BuildDone,
        Sfx::UnitReady,
        Sfx::UiClick,
        Sfx::Ack,
        Sfx::Select,
        Sfx::Denied,
        Sfx::Alert,
        Sfx::Victory,
        Sfx::Defeat,
    ];

    fn file(self) -> &'static str {
        match self {
            Sfx::ShotLight => "shot_light",
            Sfx::ShotMedium => "shot_medium",
            Sfx::ShotHeavy => "shot_heavy",
            Sfx::ShotArtillery => "shot_artillery",
            Sfx::Rocket => "rocket_launch",
            Sfx::ImpactGround => "impact_ground",
            Sfx::ImpactArmor => "impact_armor",
            Sfx::ExplosionSmall => "explosion_small",
            Sfx::ExplosionBig => "explosion_big",
            Sfx::BuildPlace => "build_place",
            Sfx::BuildDone => "build_done",
            Sfx::UnitReady => "unit_ready",
            Sfx::UiClick => "ui_click",
            Sfx::Ack => "ack",
            Sfx::Select => "select",
            Sfx::Denied => "denied",
            Sfx::Alert => "alert",
            Sfx::Victory => "victory",
            Sfx::Defeat => "defeat",
        }
    }

    /// Grundlautstärke, Mindestabstand zwischen zwei Starts (s), höchstens gleichzeitig
    fn mix(self) -> (f32, f32, usize) {
        match self {
            Sfx::ShotLight => (0.35, 0.06, 4),
            Sfx::ShotMedium => (0.55, 0.05, 6),
            Sfx::ShotHeavy => (0.7, 0.08, 4),
            Sfx::ShotArtillery => (0.75, 0.15, 3),
            Sfx::Rocket => (0.5, 0.08, 5),
            Sfx::ImpactGround => (0.4, 0.05, 6),
            Sfx::ImpactArmor => (0.35, 0.05, 6),
            Sfx::ExplosionSmall => (0.75, 0.08, 5),
            Sfx::ExplosionBig => (1.0, 0.2, 3),
            Sfx::BuildPlace => (0.5, 0.1, 2),
            Sfx::BuildDone => (0.5, 0.3, 2),
            Sfx::UnitReady => (0.4, 0.25, 2),
            Sfx::UiClick => (0.35, 0.03, 3),
            Sfx::Ack => (0.4, 0.15, 2),
            Sfx::Select => (0.3, 0.08, 2),
            Sfx::Denied => (0.45, 0.3, 1),
            Sfx::Alert => (0.55, 1.0, 1),
            Sfx::Victory | Sfx::Defeat => (0.8, 1.0, 1),
        }
    }

    /// Spielgeräusche hängen vom Ort ab, Meldungen nicht
    fn positional(self) -> bool {
        (self as usize) <= Sfx::ExplosionBig as usize
    }
}

/// Einen Klang abspielen; `pos` = Ort im Spiel (leiser mit Entfernung), sonst überall gleich laut
#[derive(Message, Clone, Copy)]
pub struct PlaySfx {
    pub sfx: Sfx,
    pub pos: Option<Vec3>,
}

impl PlaySfx {
    pub fn ui(sfx: Sfx) -> Self {
        Self { sfx, pos: None }
    }
}

/// Ton- und Musik-Einstellung des Spielers (bleibt im Browser gespeichert)
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub struct SoundPrefs {
    pub sfx: bool,
    pub music: bool,
}

impl SoundPrefs {
    pub fn load() -> Self {
        let mut p = SoundPrefs { sfx: true, music: true };
        if let Some(v) = storage_get("ferrum-nova-sound") {
            p.sfx = !v.contains("sfx=0");
            p.music = !v.contains("music=0");
        }
        p
    }
    fn save(self) {
        storage_set("ferrum-nova-sound", &format!("sfx={}&music={}", self.sfx as u8, self.music as u8));
    }
}

/// localStorage, falls verfügbar (in eingebetteten Seiten kann der Zugriff verboten sein)
pub(crate) fn storage_get(_key: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()?.local_storage().ok()??.get_item(_key).ok()?
    }
    #[cfg(not(target_arch = "wasm32"))]
    None
}

pub(crate) fn storage_set(_key: &str, _value: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(_key, _value);
    }
}

#[derive(Resource)]
pub struct Sounds {
    clips: Vec<Handle<AudioSource>>,
    /// Fassung im Klang von 2000 (Zeitsprung, era.rs)
    y2000: Vec<Handle<AudioSource>>,
    /// Fassung im Klang von 1996
    retro: Vec<Handle<AudioSource>>,
}

impl Sounds {
    fn bank(&self, era: Era) -> &[Handle<AudioSource>] {
        match era {
            Era::Now => &self.clips,
            Era::Y2000 => &self.y2000,
            Era::Retro => &self.retro,
        }
    }
}

#[derive(Component)]
pub struct Voice(Sfx);

/// Musik eines Zeitalters; es spielt nur die des aktuellen
#[derive(Component)]
pub struct MusicTrack(Era);

const MUSIC_VOLUME: f32 = 0.32;

pub fn setup_audio(mut commands: Commands, assets: Res<AssetServer>, prefs: Res<SoundPrefs>, era: Res<Era>) {
    let load = |dir: &str| Sfx::ALL.iter().map(|s| assets.load(format!("sounds/{dir}{}.ogg", s.file()))).collect();
    commands.insert_resource(Sounds { clips: load(""), y2000: load("y2000/"), retro: load("retro/") });
    let tracks = [(Era::Now, "sounds/music.ogg"), (Era::Y2000, "sounds/y2000/music.ogg"), (Era::Retro, "sounds/retro/music.ogg")];
    for (track, file) in tracks {
        commands.spawn((
            AudioPlayer::new(assets.load(file)),
            PlaybackSettings {
                mode: PlaybackMode::Loop,
                volume: Volume::Linear(MUSIC_VOLUME),
                paused: !prefs.music || track != *era,
                ..default()
            },
            MusicTrack(track),
        ));
    }
}

/// Effekt-Ereignisse in Geräusche übersetzen
pub fn fx_sounds(mut fx: MessageReader<FxEvent>, mut out: MessageWriter<PlaySfx>) {
    for ev in fx.read() {
        let (sfx, pos) = match *ev {
            FxEvent::Muzzle { pos, caliber, .. } => (
                match caliber {
                    Caliber::Light => Sfx::ShotLight,
                    Caliber::Medium => Sfx::ShotMedium,
                    Caliber::Heavy => Sfx::ShotHeavy,
                    Caliber::Artillery => Sfx::ShotArtillery,
                    Caliber::Rocket => Sfx::Rocket,
                },
                pos,
            ),
            FxEvent::Launch { pos, .. } => (Sfx::Rocket, pos),
            FxEvent::Impact { pos, caliber, surface } => (
                match (caliber, surface) {
                    (Caliber::Artillery | Caliber::Rocket, _) => Sfx::ExplosionSmall,
                    (_, Surface::Armor) => Sfx::ImpactArmor,
                    _ => Sfx::ImpactGround,
                },
                pos,
            ),
            FxEvent::Death { pos, big, .. } => (if big { Sfx::ExplosionBig } else { Sfx::ExplosionSmall }, pos),
            _ => continue,
        };
        out.write(PlaySfx { sfx, pos: Some(pos) });
    }
}

/// Meldungen an den Spieler: Bau begonnen/fertig, Einheit bereit, Befehl bestätigt, Alarm, Spielende
pub fn game_sounds(
    mut out: MessageWriter<PlaySfx>,
    placed: Query<&Team, Added<Construction>>,
    mut finished: RemovedComponents<Construction>,
    buildings: Query<&Team, With<Building>>,
    new_units: Query<&Team, Added<Unit>>,
    markers: Query<(), Added<crate::orders::Marker>>,
    selected: Query<&Team, Added<Selected>>,
    alerts: Res<crate::orders::Alerts>,
    outcome: Res<Outcome>,
    mut state: Local<(bool, usize, Option<Outcome>)>,
) {
    let (started, pings, last_outcome) = &mut *state;
    if placed.iter().any(|t| *t == crate::net::me()) {
        out.write(PlaySfx::ui(Sfx::BuildPlace));
    }
    if finished.read().any(|e| buildings.get(e).is_ok_and(|t| *t == crate::net::me())) {
        out.write(PlaySfx::ui(Sfx::BuildDone));
    }
    // Die Startarmee meldet sich nicht
    if *started && new_units.iter().any(|t| *t == crate::net::me()) {
        out.write(PlaySfx::ui(Sfx::UnitReady));
    }
    *started = true;
    if !markers.is_empty() {
        out.write(PlaySfx::ui(Sfx::Ack));
    } else if selected.iter().any(|t| *t == crate::net::me()) {
        out.write(PlaySfx::ui(Sfx::Select));
    }
    if alerts.pings.len() > *pings {
        out.write(PlaySfx::ui(Sfx::Alert));
    }
    *pings = alerts.pings.len();
    if *last_outcome != Some(*outcome) {
        match *outcome {
            Outcome::Won => {
                out.write(PlaySfx::ui(Sfx::Victory));
            }
            Outcome::Lost => {
                out.write(PlaySfx::ui(Sfx::Defeat));
            }
            Outcome::Running => {}
        }
        *last_outcome = Some(*outcome);
    }
}

/// Klänge starten: Entfernung, Mindestabstand, Stimmenzahl
pub fn play_sounds(
    mut commands: Commands,
    mut requests: MessageReader<PlaySfx>,
    sounds: Option<Res<Sounds>>,
    prefs: Res<SoundPrefs>,
    rig: Res<CamRig>,
    time: Res<Time<Real>>,
    voices: Query<&Voice>,
    mut rng: ResMut<Rng>,
    era: Res<Era>,
    mut last: Local<Vec<f32>>,
) {
    let Some(sounds) = sounds else { return };
    if !prefs.sfx {
        requests.clear();
        return;
    }
    if last.is_empty() {
        last.resize(Sfx::ALL.len(), -10.0);
    }
    let now = time.elapsed_secs();
    let mut active = vec![0usize; Sfx::ALL.len()];
    for v in &voices {
        active[v.0 as usize] += 1;
    }
    let mut total: usize = active.iter().sum();
    for r in requests.read() {
        let i = r.sfx as usize;
        let (base, gap, max) = r.sfx.mix();
        if now - last[i] < gap || active[i] >= max || total >= 24 {
            continue;
        }
        let mut volume = base;
        if r.sfx.positional() {
            let Some(p) = r.pos else { continue };
            // Hörer schwebt über dem Blickpunkt; weiter herausgezoomt hört man alles etwas leiser
            let d = flat(p).distance(flat(rig.focus));
            let reach = 22.0 + rig.dist * 0.25;
            volume *= 1.0 / (1.0 + (d / reach).powi(2)) * (1.15 - rig.dist / 220.0);
            if volume < 0.03 {
                continue;
            }
        }
        last[i] = now;
        active[i] += 1;
        total += 1;
        let speed = if r.sfx.positional() { rng.range(0.92, 1.08) } else { 1.0 };
        commands.spawn((
            AudioPlayer::new(sounds.bank(*era)[i].clone()),
            PlaybackSettings { mode: PlaybackMode::Despawn, volume: Volume::Linear(volume), speed, ..default() },
            Voice(r.sfx),
        ));
    }
}

/// Musik an/aus (und die des aktuellen Zeitalters) übernehmen und Einstellung speichern
pub fn apply_prefs(prefs: Res<SoundPrefs>, era: Res<Era>, music: Query<(&AudioSink, &MusicTrack)>, mut saved: Local<Option<SoundPrefs>>) {
    for (sink, track) in &music {
        let on = prefs.music && track.0 == *era;
        if on && sink.is_paused() {
            sink.play();
        } else if !on && !sink.is_paused() {
            sink.pause();
        }
    }
    if saved.is_some_and(|s| s != *prefs) {
        prefs.save();
    }
    *saved = Some(*prefs);
}
