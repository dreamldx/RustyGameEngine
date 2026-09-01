use bevy::prelude::*;
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
    ValidateTuning,
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
        InputMap::default()
            .with(DebugAction::ToggleDebugUi, KeyCode::F1)
            .with(DebugAction::ValidateTuning, KeyCode::F5),
    ));
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
