//! Ferrum Nova – ein kleines Echtzeitstrategie-Spiel in Rust + Bevy.
//! Läuft nativ (Windows/macOS/Linux) und als WebAssembly im Browser.

// In Bevy üblich: Systeme mit vielen Parametern und komplexen Query-Typen
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod ai;
mod buildings;
mod components;
mod construction;
mod fx;
mod hud;
mod input;
mod models;
mod resources;
mod splat;
mod terrain;
mod units;
mod vegetation;
mod vehicles;
mod world;
#[cfg(test)]
mod sim_tests;

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
        }).set(AssetPlugin {
            // Im Browser keine .meta-Dateien anfragen (spart 404-Anfragen)
            meta_check: bevy::asset::AssetMetaCheck::Never,
            ..default()
        }))
        .add_plugins(MaterialPlugin::<splat::TerrainMaterial>::default())
        .add_plugins(MaterialPlugin::<vegetation::WindMaterial>::default())
        .add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default())
        .insert_resource(ClearColor(input::HAZE))
        .insert_resource(bevy::light::DirectionalLightShadowMap { size: 4096 })
        .insert_resource(Rng(0x2545_F491_4F6C_DD1D))
        .insert_resource(Stock([[200, 50], [200, 50]]))
        .init_resource::<Outcome>()
        .init_resource::<units::Snapshot>()
        .init_resource::<input::CamRig>()
        .init_resource::<input::DragState>()
        .init_resource::<ai::AiState>()
        .init_resource::<construction::Placing>()
        .add_message::<units::BuildRequest>()
        .add_message::<construction::PlaceRequest>()
        .add_message::<fx::FxEvent>()
        .add_systems(Startup, (world::setup_world, input::setup_camera, hud::setup_hud, hud::setup_perf, splat::load_terrain_textures, vegetation::setup_vegetation))
        .add_systems(
            Update,
            ((
                input::camera_control,
                construction::placement_input,
                input::mouse_commands,
                input::select_army,
                hud::build_hotkeys,
                ai::enemy_ai,
                ai::ai_tactics,
                units::enqueue_builds,
                construction::place_buildings,
                construction::construct,
            )
                .chain(),
            (
                units::snapshot,
                units::auto_target,
                units::combat,
                units::movement,
                units::separation,
                units::vehicle_motion,
                units::ground_snap,
                units::turret_aim,
                construction::structure_aim,
                units::harvest,
                units::projectiles,
                units::production,
                units::repair,
                units::deaths,
                units::check_outcome,
            )
                .chain(),
            )
                .chain()
                .run_if(|o: Res<Outcome>| *o == Outcome::Running),
        )
        .add_systems(Update, (input::draw_overlays, hud::build_buttons, hud::update_hud, hud::scale_ui, hud::update_perf, splat::finish_terrain))
        // Optik: läuft auch nach Spielende weiter, damit Explosionen ausklingen
        .add_systems(
            Update,
            (
                fx::damage_smoke,
                fx::spawn_effects,
                fx::update_fx,
                fx::update_wrecks,
                fx::update_collapse,
                fx::ambient_fx,
                units::spin_wheels,
                units::vehicle_visuals,
                models::pulse_lights,
                buildings::spin,
                resources::shrink_nodes,
                resources::rise,
                input::update_shadows,
            )
                .chain(),
        )
        .run();
}
