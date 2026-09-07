pub mod camera;
pub mod components;
pub mod debug_ui;
pub mod input;
pub mod level;
pub mod player;
pub mod scripting;
pub mod systems;

use bevy::diagnostic::{
    EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
    SystemInformationDiagnosticsPlugin,
};
use bevy::prelude::*;
use bevy::reflect::TypeRegistration;
use bevy_egui::EguiPlugin;
use bevy_mod_scripting::prelude::*;
use bevy_rapier2d::prelude::{NoUserData, RapierPhysicsPlugin};
use input::{DebugAction, PlayerAction};
use leafwing_input_manager::prelude::InputManagerPlugin;

pub struct EnginePlugin;

/// Set once the current level's `on_level_load()` callback has responded, signaling
/// that `PlayerSpawn` is populated and it's safe to spawn the player (and,
/// the first time, the camera). See `level::handle_level_load_response` /
/// `player::spawn_player`.
#[derive(Resource, Default)]
pub struct ReadyToPlay(pub bool);

/// `CoreScriptGlobalsPlugin` exposes every type in the `TypeRegistry` to
/// scripts as a global, keyed by its bare type name (generic parameters and
/// module path stripped). Several types the engine pulls in via Bevy/leafwing
/// dependencies collapse onto the same bare name and would otherwise spam
/// "Duplicate global registration" warnings on every startup:
/// - `ArcMutexValue<T>` / `Arc<T>`: bevy_mod_scripting's own internal asset
///   wrappers, one instantiation per asset type. Never meant to be named
///   from scripts.
/// - `Range<T>`: same story for the several `core::ops::Range<_>` reflected
///   by bevy's asset/animation types.
/// - `Sphere`, `Button`, `ButtonState`: unrelated types from different
///   crates (bevy_math vs bevy_camera, bevy_ui vs bevy_ui_widgets,
///   bevy_input vs leafwing_input_manager) that happen to share a bare name.
///   We keep the definition scripts are actually meant to see and drop the
///   other.
fn script_globals_filter(registration: &TypeRegistration) -> bool {
    let path = registration.type_info().type_path();
    !(path.starts_with("bevy_asset::handle::ArcMutexValue<")
        || path.starts_with("bevy_platform::sync::Arc<")
        || path.starts_with("core::ops::Range<")
        || path == "bevy_camera::primitives::Sphere"
        || path == "bevy_ui::widget::button::Button"
        || path == "leafwing_input_manager::buttonlike::ButtonState")
}

impl Plugin for EnginePlugin {
    fn build(&self, app: &mut App) {
        scripting::register_functions(app.world_mut());
        level::register_functions(app.world_mut());

        app.add_plugins(BMSPlugin.build().set(CoreScriptGlobalsPlugin {
            filter: script_globals_filter,
            ..default()
        }))

            .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
            .add_plugins(EguiPlugin::default())
            .add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_plugins(EntityCountDiagnosticsPlugin::default())
            .add_plugins(SystemInformationDiagnosticsPlugin::default())
            .add_plugins(InputManagerPlugin::<PlayerAction>::default())
            .add_plugins(InputManagerPlugin::<DebugAction>::default())
            .add_plugins(systems::GameplaySystemsPlugin);
    }
}
