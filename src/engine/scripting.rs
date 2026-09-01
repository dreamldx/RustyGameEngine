use bevy::prelude::*;
use bevy_mod_scripting::prelude::*;
use bevy_mod_scripting_bindings::{FunctionCallContext, InteropError};
use bevy_mod_scripting_core::event::ScriptCallbackResponseEvent;
use crate::engine::input::{DebugAction, DebugInputMarker};
use leafwing_input_manager::prelude::*;
use std::fs;
use std::path::Path;

#[derive(Resource)]
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

/// Marks the script entity loaded from `assets/scripts/player.lua`, so the
/// F5 debug validation system can find it specifically among any other
/// scripts `load_scripts` discovers.
#[derive(Component)]
pub struct PlayerTuningScript;

#[script_bindings(remote, unregistered)]
impl World {
    /// Called from `player.lua`'s `on_script_loaded` as `world.set_player_tuning(...)`.
    pub fn set_player_tuning(
        context: FunctionCallContext,
        move_speed: f32,
        jump_force: f32,
        gravity: f32,
    ) -> Result<(), InteropError> {
        let world = context.world()?;
        world.with_world_mut_access(|world| {
            if let Some(mut tuning) = world.get_resource_mut::<ScriptedTuning>() {
                tuning.move_speed = move_speed;
                tuning.jump_force = jump_force;
                tuning.gravity = gravity;
            }
        })?;
        Ok(())
    }
}

/// Scans `assets/scripts/` for every `.lua` file, loads it, and spawns a
/// script entity per file. The entity for `player.lua` is additionally
/// marked `PlayerTuningScript`. Loading is directory-driven so any future
/// `.lua` file dropped into this folder is picked up automatically.
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
        let mut entity = commands.spawn(ScriptComponent(vec![handle]));
        if file_name == "player.lua" {
            entity.insert(PlayerTuningScript);
        }
    }
}

callback_labels!(OnGetTuning => "get_tuning");

/// On `ValidateTuning`, sends a `get_tuning` callback request to the player
/// tuning script. The response is handled by `handle_tuning_validation_response`.
pub fn request_tuning_validation(
    debug_input: Query<&ActionState<DebugAction>, With<DebugInputMarker>>,
    query: Query<(Entity, &ScriptComponent), With<PlayerTuningScript>>,
    mut callbacks: MessageWriter<ScriptCallbackEvent>,
) {
    let Ok(debug_action_state) = debug_input.single() else {
        return;
    };
    if !debug_action_state.just_pressed(&DebugAction::ValidateTuning) {
        return;
    }
    let Ok((entity, script)) = query.single() else {
        warn!("ValidateTuning pressed but no player tuning script is loaded yet");
        return;
    };
    let Some(handle) = script.0.first() else {
        return;
    };
    callbacks.write(
        ScriptCallbackEvent::new_for_script_entity(OnGetTuning, vec![], handle.clone(), entity)
            .with_response(),
    );
}

/// Compares a script's reported tuning against the live `ScriptedTuning`
/// resource. Returns `None` when they agree, or `Some(description)` naming
/// the mismatched field(s) otherwise.
pub fn describe_tuning_mismatch(current: &ScriptedTuning, queried: (f32, f32, f32)) -> Option<String> {
    let (move_speed, jump_force, gravity) = queried;
    let mut mismatches = Vec::new();
    if (current.move_speed - move_speed).abs() > f32::EPSILON {
        mismatches.push(format!(
            "move_speed: resource={} script={}",
            current.move_speed, move_speed
        ));
    }
    if (current.jump_force - jump_force).abs() > f32::EPSILON {
        mismatches.push(format!(
            "jump_force: resource={} script={}",
            current.jump_force, jump_force
        ));
    }
    if (current.gravity - gravity).abs() > f32::EPSILON {
        mismatches.push(format!(
            "gravity: resource={} script={}",
            current.gravity, gravity
        ));
    }
    if mismatches.is_empty() {
        None
    } else {
        Some(mismatches.join(", "))
    }
}

fn script_value_to_f32(value: &ScriptValue) -> Option<f32> {
    match value {
        ScriptValue::Float(f) => Some(*f as f32),
        ScriptValue::Integer(i) => Some(*i as f32),
        _ => None,
    }
}

fn extract_three_floats(value: &ScriptValue) -> Option<(f32, f32, f32)> {
    let ScriptValue::Map(map) = value else {
        return None;
    };
    let move_speed = script_value_to_f32(map.get("move_speed")?)?;
    let jump_force = script_value_to_f32(map.get("jump_force")?)?;
    let gravity = script_value_to_f32(map.get("gravity")?)?;
    Some((move_speed, jump_force, gravity))
}

/// Reads `get_tuning` responses and logs whether the script agrees with
/// `ScriptedTuning`.
pub fn handle_tuning_validation_response(
    tuning: Res<ScriptedTuning>,
    mut responses: MessageReader<ScriptCallbackResponseEvent>,
) {
    for response in responses.read() {
        if response.label != OnGetTuning.into() {
            continue;
        }
        let queried = match &response.response {
            Ok(value) => match extract_three_floats(value) {
                Some(v) => v,
                None => {
                    warn!("get_tuning() returned an unexpected shape: {value:?}");
                    continue;
                }
            },
            Err(e) => {
                warn!("get_tuning() callback failed: {e}");
                continue;
            }
        };
        match describe_tuning_mismatch(&tuning, queried) {
            None => info!("Scripted tuning matches ScriptedTuning resource"),
            Some(diff) => warn!("Scripted tuning mismatch: {diff}"),
        }
    }
}

#[cfg(test)]
mod tuning_validation_tests {
    use super::*;

    #[test]
    fn describe_tuning_mismatch_returns_none_when_equal() {
        let tuning = ScriptedTuning {
            move_speed: 200.0,
            jump_force: 500.0,
            gravity: 980.0,
        };
        assert_eq!(
            describe_tuning_mismatch(&tuning, (200.0, 500.0, 980.0)),
            None
        );
    }

    #[test]
    fn describe_tuning_mismatch_reports_differing_fields() {
        let tuning = ScriptedTuning {
            move_speed: 200.0,
            jump_force: 500.0,
            gravity: 980.0,
        };
        let msg = describe_tuning_mismatch(&tuning, (250.0, 500.0, 1000.0))
            .expect("expected a mismatch description");
        assert!(msg.contains("move_speed"));
        assert!(msg.contains("gravity"));
        assert!(!msg.contains("jump_force"));
    }

    #[test]
    fn request_tuning_validation_sends_callback_on_validate_action() {
        use crate::engine::input::{DebugAction, DebugInputMarker};
        use bevy::input::InputPlugin;
        use bevy_mod_scripting::prelude::ScriptAsset;
        use bevy_mod_scripting_core::event::CallbackLabel;
        use leafwing_input_manager::prelude::*;

        #[derive(Resource, Default)]
        struct CapturedCallbacks(Vec<CallbackLabel>);

        fn capture_callbacks(
            mut reader: MessageReader<ScriptCallbackEvent>,
            mut captured: ResMut<CapturedCallbacks>,
        ) {
            for event in reader.read() {
                captured.0.push(event.label.clone());
            }
        }

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputPlugin,
            InputManagerPlugin::<DebugAction>::default(),
        ));
        app.add_message::<ScriptCallbackEvent>();
        app.insert_resource(CapturedCallbacks::default());

        app.world_mut().spawn((
            DebugInputMarker,
            InputMap::default().with(DebugAction::ValidateTuning, KeyCode::F5),
        ));

        let mut script_assets = Assets::<ScriptAsset>::default();
        let handle = script_assets.add(ScriptAsset::new(String::new()));
        app.insert_resource(script_assets);
        app.world_mut()
            .spawn((ScriptComponent(vec![handle]), PlayerTuningScript));

        app.add_systems(
            Update,
            (request_tuning_validation, capture_callbacks).chain(),
        );

        KeyCode::F5.press(app.world_mut());
        app.update();

        let captured = app.world().resource::<CapturedCallbacks>();
        assert_eq!(captured.0, vec![OnGetTuning.into()]);
    }
}
