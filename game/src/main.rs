//! Ferrum Nova – ein kleines Echtzeitstrategie-Spiel in Rust + Bevy.
//! Läuft nativ (Windows/macOS/Linux) und als WebAssembly im Browser.

// In Bevy üblich: Systeme mit vielen Parametern und komplexen Query-Typen
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod ai;
mod components;
mod hud;
mod input;
mod units;
mod world;

use bevy::prelude::*;
use components::*;

fn main() {
    App::new()
        .set_error_handler(bevy::ecs::error::warn)
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Ferrum Nova".into(),
                // Im Browser: an das <canvas id="game"> der Webseite binden
                canvas: Some("#game".into()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: true,
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.05, 0.06, 0.08)))
        .insert_resource(Stock([[200, 50], [200, 50]]))
        .init_resource::<Outcome>()
        .init_resource::<units::Snapshot>()
        .init_resource::<input::CamRig>()
        .init_resource::<input::DragState>()
        .init_resource::<ai::AiState>()
        .add_message::<units::BuildRequest>()
        .add_systems(Startup, (world::setup_world, input::setup_camera, hud::setup_hud))
        .add_systems(
            Update,
            (
                input::camera_control,
                input::mouse_commands,
                input::select_army,
                hud::build_hotkeys,
                ai::enemy_ai,
                units::enqueue_builds,
                units::snapshot,
                units::auto_target,
                units::combat,
                units::movement,
                units::separation,
                units::harvest,
                units::projectiles,
                units::production,
                units::deaths,
                units::check_outcome,
            )
                .chain()
                .run_if(|o: Res<Outcome>| *o == Outcome::Running),
        )
        .add_systems(Update, (input::draw_overlays, hud::build_buttons, hud::update_hud, hud::scale_ui))
        .run();
}
