use bevy::prelude::*;
use bevy_mod_scripting::prelude::*;
use bevy_mod_scripting_bindings::{FunctionCallContext, InteropError, ScriptValue};
use crate::engine::components::Player;
use std::fs;
use std::path::Path;

#[derive(Component)]
pub struct ScriptedTuning {
    pub move_speed: f32,
    pub jump_force: f32,
    pub gravity: f32,
}

impl Default for ScriptedTuning {
    fn default() -> Self {
        Self {
            move_speed: 200.0,
            jump_force: 500.0,
            gravity: 980.0,
        }
    }
}

#[script_bindings(remote, unregistered)]
impl World {
    /// Called from `player.lua`'s `on_script_loaded`/`on_script_reloaded` as
    /// `world.set_player_tuning(...)`. Lua is the source of truth for these
    /// values; this just copies them onto the player's `ScriptedTuning`.
    pub fn set_player_tuning(
        context: FunctionCallContext,
        move_speed: f32,
        jump_force: f32,
        gravity: f32,
    ) -> Result<(), InteropError> {
        let world = context.world()?;
        world.with_world_mut_access(|world| {
            let mut tuning_query = world.query_filtered::<&mut ScriptedTuning, With<Player>>();
            if let Ok(mut tuning) = tuning_query.single_mut(world) {
                tuning.move_speed = move_speed;
                tuning.jump_force = jump_force;
                tuning.gravity = gravity;
            }
        })?;
        Ok(())
    }

    /// Called from `config/player_input.lua`'s `on_script_loaded`/`on_script_reloaded`
    /// as `world.set_player_input(config)`. A malformed config is rejected with
    /// a warning and the player's current bindings stay in effect.
    pub fn set_player_input(
        context: FunctionCallContext,
        config: ScriptValue,
    ) -> Result<(), InteropError> {
        let world = context.world()?;
        world.with_world_mut_access(|world| {
            match crate::engine::input::parse_input_config(&config) {
                Ok(map) => {
                    if let Some(mut pending) =
                        world.get_resource_mut::<crate::engine::input::PendingPlayerInputMap>()
                    {
                        pending.0 = Some(map);
                    }
                }
                Err(e) => warn!("player_input.lua config rejected, keeping current bindings: {e}"),
            }
        })?;
        Ok(())
    }
}

/// Scans `assets/scripts/` for every `.lua` file, loads it, and spawns a
/// script entity per file. Loading is directory-driven so any future `.lua`
/// file dropped into this folder is picked up automatically.
pub fn load_scripts(asset_server: Res<AssetServer>, mut commands: Commands) {
    let dir = Path::new("assets/scripts");
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            error!("Failed to read scripts directory {}: {}", dir.display(), e);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lua") {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|f| f.to_str()) else {
            continue;
        };

        let handle = asset_server.load::<ScriptAsset>(format!("scripts/{file_name}"));
        commands.spawn(ScriptComponent(vec![handle]));
    }
}
