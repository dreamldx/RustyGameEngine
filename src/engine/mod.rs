pub mod components;
pub mod debug_ui;
pub mod input;
pub mod level;
pub mod player;
pub mod scripting;
pub mod systems;

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use bevy_mod_scripting::prelude::*;
use debug_ui::{ReloadLevelRequested, UiVisible};
use input::{DebugAction, PlayerAction};
use leafwing_input_manager::prelude::InputManagerPlugin;
use scripting::ScriptedTuning;

pub struct EnginePlugin;

impl Plugin for EnginePlugin {
    fn build(&self, app: &mut App) {
        scripting::register_functions(app.world_mut());

        app.add_plugins(BMSPlugin)
            .add_plugins(EguiPlugin::default())
            .add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_plugins(InputManagerPlugin::<PlayerAction>::default())
            .add_plugins(InputManagerPlugin::<DebugAction>::default())
            .init_resource::<ScriptedTuning>()
            .init_resource::<UiVisible>()
            .init_resource::<ReloadLevelRequested>()
            .init_resource::<debug_ui::AssetTree>()
            .add_systems(
                Startup,
                (
                    level::spawn_level,
                    player::spawn_player,
                    player::spawn_camera,
                    scripting::load_scripts,
                    debug_ui::build_asset_tree,
                    input::spawn_debug_input_map,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    systems::player_input,
                    systems::apply_gravity,
                    systems::ground_detection,
                    systems::apply_velocity,
                    systems::camera_follow,
                ).chain(),
            )
            .add_systems(
                Update,
                (
                    scripting::request_tuning_validation,
                    scripting::handle_tuning_validation_response,
                    event_handler::<scripting::OnGetTuning, LuaScriptingPlugin>,
                ),
            )
            .add_systems(
                Update,
                (debug_ui::toggle_ui_visibility, debug_ui::apply_level_reload),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (debug_ui::draw_debug_ui, debug_ui::draw_panels_ui),
            );
    }
}
