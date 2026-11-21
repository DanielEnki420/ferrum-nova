//! Ferrum Nova – ein kleines Echtzeitstrategie-Spiel in Rust + Bevy.
//! Läuft nativ (Windows/macOS/Linux) und als WebAssembly im Browser.

// In Bevy üblich: Systeme mit vielen Parametern und komplexen Query-Typen
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod ai;
mod atmo;
mod audio;
mod buildings;
mod campaign;
mod components;
mod construction;
mod frame;
mod fog;
mod fx;
mod hud;
mod input;
mod minimap;
mod models;
mod nav;
mod orders;
mod resources;
mod sky;
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
    let config = frame::MatchConfig::load();
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
        .insert_resource(config)
        .insert_resource(audio::SoundPrefs::load())
        .add_message::<audio::PlaySfx>()
        .init_resource::<frame::Phase>()
        .init_resource::<fog::FogGrid>()
        .init_resource::<frame::Stats>()
        .init_resource::<frame::HelpShown>()
        .init_resource::<frame::CampaignShown>()
        .init_resource::<campaign::Unlocks>()
        .init_resource::<units::Snapshot>()
        .init_resource::<input::CamRig>()
        .init_resource::<input::DragState>()
        .init_resource::<input::ControlGroups>()
        .init_resource::<input::Hover>()
        .init_resource::<input::ClickMode>()
        .init_resource::<nav::NavGrid>()
        .init_resource::<nav::PathQueue>()
        .init_resource::<orders::Alerts>()
        .init_resource::<ai::AiState>()
        .init_resource::<construction::Placing>()
        .add_message::<units::BuildRequest>()
        .add_message::<construction::PlaceRequest>()
        .add_message::<fx::FxEvent>()
        .add_systems(Startup, fog::setup_fog.before(world::setup_world).before(vegetation::setup_vegetation))
        .add_systems(Startup, (world::setup_world, input::setup_camera, hud::setup_hud, hud::setup_perf, minimap::setup_minimap, splat::load_terrain_textures, vegetation::setup_vegetation, atmo::setup_atmo, frame::setup_frame, frame::autostart, audio::setup_audio))
        .add_systems(Startup, (campaign::setup_mission.after(world::setup_world), campaign::setup_mission_hud.after(hud::setup_hud)))
        .add_systems(
            Update,
            ((
                input::camera_control,
                construction::placement_input,
                input::mouse_commands,
                input::toggle_click_mode,
                input::order_hotkeys,
                input::control_groups,
                input::select_army,
                hud::build_hotkeys,
                minimap::minimap_input,
                ai::enemy_ai,
                ai::ai_tactics,
                units::enqueue_builds,
                construction::place_buildings,
                construction::construct,
            )
                .chain(),
            (
                units::snapshot,
                nav::update_grid,
                units::auto_target,
                units::combat,
                orders::follow_orders,
                nav::plan_paths,
                units::movement,
                orders::face_on_arrival,
                units::separation,
                units::vehicle_motion,
                units::ground_snap,
                units::turret_aim,
                construction::structure_aim,
                units::harvest,
                units::projectiles,
                units::production,
                units::repair,
                (frame::track_stats, units::deaths, units::check_outcome, campaign::mission_tick, orders::tick_alerts).chain(),
            )
                .chain(),
            )
                .chain()
                .run_if(frame::playing),
        )
        .add_systems(
            Update,
            (
                input::hover,
                input::draw_overlays,
                input::draw_orders,
                orders::draw_markers,
                hud::build_buttons.run_if(frame::playing),
                frame::pause_keys.before(construction::placement_input),
                frame::frame_buttons,
                frame::update_frame,
                frame::menu_camera,
                campaign::enforce_unlocks,
                frame::hide_loading,
                (fog::update_fog, fog::hide_enemies).chain(),
                (audio::fx_sounds, audio::game_sounds, audio::play_sounds, audio::apply_prefs).chain(),
                hud::update_hud,
                hud::update_selection_panel,
                hud::selection_cards,
                hud::scale_ui,
                hud::update_perf,
                minimap::update_minimap,
                splat::finish_terrain,
            ),
        )
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
                models::pulse_beacons,
                buildings::spin,
                construction::gate_doors,
                resources::shrink_nodes,
                resources::rise,
                input::update_shadows,
                input::update_focus,
                atmo::attach_field_glow,
                atmo::pulse_field_glow,
                atmo::update_dust,
            )
                .chain(),
        )
        .run();
}
