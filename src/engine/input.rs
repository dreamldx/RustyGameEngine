use crate::engine::components::Player;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_mod_scripting_bindings::ScriptValue;
use leafwing_input_manager::prelude::*;

#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect)]
pub enum PlayerAction {
    #[actionlike(Axis)]
    Move,
    Jump,
}

#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect)]
pub enum DebugAction {
    ToggleDebugUi,
}

/// The player's input bindings: `A`/`D` and the arrow keys both drive the
/// same `Move` axis (either can be held/released independently — the crate
/// merges them), `Space` triggers `Jump`.
pub fn player_input_map() -> InputMap<PlayerAction> {
    InputMap::default()
        .with_axis(PlayerAction::Move, VirtualAxis::ad())
        .with_axis(PlayerAction::Move, VirtualAxis::horizontal_arrow_keys())
        .with(PlayerAction::Jump, KeyCode::Space)
}

/// Marks the entity carrying `InputMap<DebugAction>`/`ActionState<DebugAction>`,
/// so consumers can query it specifically — `DebugAction` isn't tied to the
/// player entity, and resource-only `ActionState` isn't supported by this
/// version of the crate.
#[derive(Component)]
pub struct DebugInputMarker;

pub fn spawn_debug_input_map(mut commands: Commands) {
    commands.spawn((
        DebugInputMarker,
        InputMap::default().with(DebugAction::ToggleDebugUi, KeyCode::F1),
    ));
}

pub fn parse_keycode(name: &str) -> Option<KeyCode> {
    use KeyCode::*;
    let code = match name {
        "KeyA" => KeyA, "KeyB" => KeyB, "KeyC" => KeyC, "KeyD" => KeyD,
        "KeyE" => KeyE, "KeyF" => KeyF, "KeyG" => KeyG, "KeyH" => KeyH,
        "KeyI" => KeyI, "KeyJ" => KeyJ, "KeyK" => KeyK, "KeyL" => KeyL,
        "KeyM" => KeyM, "KeyN" => KeyN, "KeyO" => KeyO, "KeyP" => KeyP,
        "KeyQ" => KeyQ, "KeyR" => KeyR, "KeyS" => KeyS, "KeyT" => KeyT,
        "KeyU" => KeyU, "KeyV" => KeyV, "KeyW" => KeyW, "KeyX" => KeyX,
        "KeyY" => KeyY, "KeyZ" => KeyZ,
        "Digit0" => Digit0, "Digit1" => Digit1, "Digit2" => Digit2,
        "Digit3" => Digit3, "Digit4" => Digit4, "Digit5" => Digit5,
        "Digit6" => Digit6, "Digit7" => Digit7, "Digit8" => Digit8,
        "Digit9" => Digit9,
        "F1" => F1, "F2" => F2, "F3" => F3, "F4" => F4, "F5" => F5,
        "F6" => F6, "F7" => F7, "F8" => F8, "F9" => F9, "F10" => F10,
        "F11" => F11, "F12" => F12,
        "ArrowLeft" => ArrowLeft, "ArrowRight" => ArrowRight,
        "ArrowUp" => ArrowUp, "ArrowDown" => ArrowDown,
        "Space" => Space, "Enter" => Enter, "Escape" => Escape,
        "Tab" => Tab, "Backspace" => Backspace,
        "ShiftLeft" => ShiftLeft, "ShiftRight" => ShiftRight,
        "ControlLeft" => ControlLeft, "ControlRight" => ControlRight,
        "AltLeft" => AltLeft, "AltRight" => AltRight,
        "Comma" => Comma, "Period" => Period, "Slash" => Slash,
        "Semicolon" => Semicolon, "Quote" => Quote, "Backslash" => Backslash,
        "BracketLeft" => BracketLeft, "BracketRight" => BracketRight,
        "Minus" => Minus, "Equal" => Equal, "Backquote" => Backquote,
        _ => return None,
    };
    Some(code)
}

fn axis_key(axis: &HashMap<String, ScriptValue>, field: &str) -> Result<KeyCode, String> {
    let Some(ScriptValue::String(name)) = axis.get(field) else {
        return Err(format!("axis entry is missing string field '{field}'"));
    };
    parse_keycode(name).ok_or_else(|| format!("unknown key name '{name}'"))
}

pub fn parse_input_config(value: &ScriptValue) -> Result<InputMap<PlayerAction>, String> {
    let ScriptValue::Map(config) = value else {
        return Err("input config must be a table".to_string());
    };
    let Some(ScriptValue::Map(move_config)) = config.get("move") else {
        return Err("config.move must be a table".to_string());
    };
    let Some(ScriptValue::List(axes)) = move_config.get("axes") else {
        return Err("config.move.axes must be a list of {neg, pos} tables".to_string());
    };
    let Some(ScriptValue::List(jump)) = config.get("jump") else {
        return Err("config.jump must be a list of key names".to_string());
    };

    let mut map = InputMap::default();
    for axis in axes {
        let ScriptValue::Map(axis) = axis else {
            return Err("each config.move.axes entry must be a {neg, pos} table".to_string());
        };
        map = map.with_axis(
            PlayerAction::Move,
            VirtualAxis::new(axis_key(axis, "neg")?, axis_key(axis, "pos")?),
        );
    }
    for key in jump {
        let ScriptValue::String(name) = key else {
            return Err("config.jump entries must be key name strings".to_string());
        };
        let key = parse_keycode(name).ok_or_else(|| format!("unknown key name '{name}'"))?;
        map = map.with(PlayerAction::Jump, key);
    }
    Ok(map)
}

#[derive(Resource, Default)]
pub struct PendingPlayerInputMap(pub Option<InputMap<PlayerAction>>);

pub fn apply_player_input_map(
    mut pending: ResMut<PendingPlayerInputMap>,
    mut player: Query<&mut InputMap<PlayerAction>, With<Player>>,
) {
    if pending.0.is_none() {
        return;
    }
    let Ok(mut map) = player.single_mut() else {
        return;
    };
    *map = pending.0.take().expect("checked is_none above");
    info!("Applied player input map from player_input.lua");
}

#[cfg(test)]
mod tests { 
    use super::*;
    use bevy::input::InputPlugin;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputPlugin,
            InputManagerPlugin::<PlayerAction>::default(),
        ));
        app.world_mut().spawn(player_input_map());
        app.update();
        app
    }

    fn action_state(app: &mut App) -> ActionState<PlayerAction> {
        let world = app.world_mut();
        let mut query = world.query::<&ActionState<PlayerAction>>();
        query.single(world).unwrap().clone()
    }

    #[test]
    fn move_axis_merges_ad_and_arrow_key_bindings() {
        let mut app = test_app();

        KeyCode::KeyD.press(app.world_mut());
        app.update();
        assert!(action_state(&mut app).clamped_value(&PlayerAction::Move) > 0.0);

        KeyCode::ArrowRight.press(app.world_mut());
        app.update();
        assert!(action_state(&mut app).clamped_value(&PlayerAction::Move) > 0.0);

        KeyCode::KeyD.release(app.world_mut());
        app.update();
        assert!(
            action_state(&mut app).clamped_value(&PlayerAction::Move) > 0.0,
            "releasing KeyD should not stop movement while ArrowRight is still held"
        );

        KeyCode::ArrowRight.release(app.world_mut());
        app.update();
        assert_eq!(
            action_state(&mut app).clamped_value(&PlayerAction::Move),
            0.0
        );
    }

    #[test]
    fn jump_action_detects_just_pressed() {
        let mut app = test_app();

        KeyCode::Space.press(app.world_mut());
        app.update();

        assert!(action_state(&mut app).just_pressed(&PlayerAction::Jump));
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;
    use crate::engine::components::Player;
    use bevy::input::InputPlugin;
    use bevy_mod_scripting_bindings::ScriptValue;

    fn sv_str(s: &str) -> ScriptValue {
        ScriptValue::String(s.to_string().into())
    }

    fn sv_map(entries: Vec<(&str, ScriptValue)>) -> ScriptValue {
        ScriptValue::Map(
            entries
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect::<HashMap<_, _>>(),
        )
    }

    fn sv_list(items: Vec<ScriptValue>) -> ScriptValue {
        ScriptValue::List(items.into_iter().collect())
    }

    fn valid_config() -> ScriptValue {
        sv_map(vec![
            (
                "move",
                sv_map(vec![(
                    "axes",
                    sv_list(vec![
                        sv_map(vec![("neg", sv_str("KeyA")), ("pos", sv_str("KeyD"))]),
                        sv_map(vec![
                            ("neg", sv_str("ArrowLeft")),
                            ("pos", sv_str("ArrowRight")),
                        ]),
                    ]),
                )]),
            ),
            ("jump", sv_list(vec![sv_str("Space")])),
        ])
    }

    #[test]
    fn parse_keycode_maps_known_names() {
        assert_eq!(parse_keycode("Space"), Some(KeyCode::Space));
        assert_eq!(parse_keycode("KeyA"), Some(KeyCode::KeyA));
        assert_eq!(parse_keycode("Digit3"), Some(KeyCode::Digit3));
        assert_eq!(parse_keycode("ArrowLeft"), Some(KeyCode::ArrowLeft));
        assert_eq!(parse_keycode("F5"), Some(KeyCode::F5));
        assert_eq!(parse_keycode("Bogus"), None);
    }

    #[test]
    fn parse_input_config_builds_map_from_valid_table() {
        let map = parse_input_config(&valid_config()).expect("valid config should parse");
        assert_eq!(map, player_input_map());
    }

    #[test]
    fn parse_input_config_rejects_missing_jump() {
        let config = sv_map(vec![(
            "move",
            sv_map(vec![(
                "axes",
                sv_list(vec![sv_map(vec![
                    ("neg", sv_str("KeyA")),
                    ("pos", sv_str("KeyD")),
                ])]),
            )]),
        )]);
        let err = parse_input_config(&config).expect_err("missing jump should be an error");
        assert!(err.contains("jump"), "error should name the missing field: {err}");
    }

    #[test]
    fn parse_input_config_rejects_unknown_key_name() {
        let config = sv_map(vec![
            (
                "move",
                sv_map(vec![(
                    "axes",
                    sv_list(vec![sv_map(vec![
                        ("neg", sv_str("Bogus")),
                        ("pos", sv_str("KeyD")),
                    ])]),
                )]),
            ),
            ("jump", sv_list(vec![sv_str("Space")])),
        ]);
        let err = parse_input_config(&config).expect_err("unknown key should be an error");
        assert!(err.contains("Bogus"), "error should name the bad key: {err}");
    }

    #[test]
    fn parse_input_config_rejects_non_map_value() {
        assert!(parse_input_config(&sv_str("nope")).is_err());
    }

    fn apply_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputPlugin,
            InputManagerPlugin::<PlayerAction>::default(),
        ));
        app.init_resource::<PendingPlayerInputMap>();
        app.add_systems(Update, apply_player_input_map);
        app
    }

    #[test]
    fn apply_player_input_map_overwrites_player_map_and_clears_pending() {
        let mut app = apply_test_app();
        let player = app.world_mut().spawn((Player, player_input_map())).id();

        let new_map = InputMap::default().with(PlayerAction::Jump, KeyCode::KeyJ);
        app.world_mut().resource_mut::<PendingPlayerInputMap>().0 = Some(new_map.clone());
        app.update();

        assert_eq!(
            app.world().get::<InputMap<PlayerAction>>(player).unwrap(),
            &new_map
        );
        assert!(app.world().resource::<PendingPlayerInputMap>().0.is_none());
    }

    #[test]
    fn apply_player_input_map_keeps_pending_until_player_exists() {
        let mut app = apply_test_app();

        let new_map = InputMap::default().with(PlayerAction::Jump, KeyCode::KeyJ);
        app.world_mut().resource_mut::<PendingPlayerInputMap>().0 = Some(new_map);
        app.update();

        assert!(
            app.world().resource::<PendingPlayerInputMap>().0.is_some(),
            "pending map must survive until the player entity is spawned"
        );
    }
}
