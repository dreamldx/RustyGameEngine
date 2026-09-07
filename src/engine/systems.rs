use crate::engine::ReadyToPlay;
use crate::engine::components::*;
use crate::engine::debug_ui::{self, ReloadLevelRequested};
use crate::engine::input::PlayerAction;
use crate::engine::level;
use crate::engine::scripting::{self, ScriptedTuning};
use crate::engine::{camera, player};
use bevy::prelude::*;
use bevy_rapier2d::prelude::{KinematicCharacterController, KinematicCharacterControllerOutput};
use leafwing_input_manager::prelude::*;

pub struct GameplaySystemsPlugin;

impl Plugin for GameplaySystemsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReloadLevelRequested>()
            .init_resource::<ReadyToPlay>()
            .add_plugins((
                player::PlayerPlugin,
                camera::CameraPlugin,
                level::LevelLoadPlugin,
                debug_ui::DebugUiPlugin,
            ))
            .add_systems(
                Startup,
                (level::load_level_scripts, scripting::load_scripts).chain(),
            )
            .add_systems(
                Update,
                (
                    sync_grounded_from_kcc,
                    player_input,
                    apply_gravity,
                    move_player_kcc,
                )
                    .chain(),
            )
            .add_systems(Update, check_fall);
    }
}

pub fn player_input(
    mut query: Query<
        (&ActionState<PlayerAction>, &mut Velocity, &Grounded, &ScriptedTuning),
        With<Player>,
    >,
) {
    let Ok((action_state, mut velocity, grounded, tuning)) = query.single_mut() else {
        return;
    };

    let dir = action_state.clamped_value(&PlayerAction::Move);
    velocity.0.x = dir * tuning.move_speed;

    if action_state.just_pressed(&PlayerAction::Jump) && grounded.0 {
        velocity.0.y = tuning.jump_force;
    }
}

pub fn apply_gravity(
    time: Res<Time>,
    mut query: Query<(&mut Velocity, &ScriptedTuning), With<Player>>,
) {
    for (mut velocity, tuning) in &mut query {
        velocity.0.y -= tuning.gravity * time.delta_secs();
    }
}

/// Feeds this frame's desired movement into Rapier's character controller,
/// which resolves it via shape-casting (a continuous sweep, not a discrete
/// position update) — the player can't tunnel through geometry no matter
/// how large a single frame's movement is, unlike the old naive
/// `transform += velocity * dt` this replaced. This is also what enforces
/// the level's horizontal bounds: `level::sync_level_bounds_walls` places
/// static wall colliders at `LevelBounds::min_x`/`max_x`, so the KCC's sweep
/// stops the player there the same way it stops them on any platform — no
/// manual clamping needed here.
pub fn move_player_kcc(
    time: Res<Time>,
    mut query: Query<(&Velocity, &mut KinematicCharacterController), With<Player>>,
) {
    let Ok((velocity, mut controller)) = query.single_mut() else {
        return;
    };
    controller.translation = Some(velocity.0 * time.delta_secs());
}

/// Reads back last frame's character-controller result (Rapier resolves
/// `KinematicCharacterController.translation` and writes this output during
/// its own `PostUpdate` pass, so it reflects the *previous* frame's
/// movement) to update `Grounded` and stop accumulating fall speed once
/// landed — mirroring what the old SAT-based `ground_detection` did, just
/// sourced from Rapier's sweep result instead of a manual overlap test.
pub fn sync_grounded_from_kcc(
    mut query: Query<
        (&KinematicCharacterControllerOutput, &mut Grounded, &mut Velocity),
        With<Player>,
    >,
) {
    let Ok((output, mut grounded, mut velocity)) = query.single_mut() else {
        return;
    };
    grounded.0 = output.grounded;
    if output.grounded && velocity.0.y <= 0.0 {
        velocity.0.y = 0.0;
    }
}

/// Player y-position below which `check_fall` reloads the level. Public so
/// `debug_ui::draw_bounds_gizmo` can draw it as a reference line.
pub const FALL_DEATH_Y: f32 = -1000.0;

pub fn check_fall(
    player_query: Query<&Transform, With<Player>>,
    mut reload: ResMut<ReloadLevelRequested>,
) {
    let Ok(transform) = player_query.single() else {
        return;
    };
    if transform.translation.y < FALL_DEATH_Y {
        reload.0 = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::input::player_input_map;
    use bevy::input::InputPlugin;

    fn player_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputPlugin,
            InputManagerPlugin::<PlayerAction>::default(),
        ));
        app.add_systems(Update, player_input);
        app
    }

    #[test]
    fn player_input_uses_scripted_move_speed() {
        let mut app = player_test_app();
        let entity = app
            .world_mut()
            .spawn((
                Player,
                Velocity::default(),
                Grounded(false),
                ScriptedTuning {
                    move_speed: 250.0,
                    jump_force: 500.0,
                    gravity: 980.0,
                },
                player_input_map(),
            ))
            .id();

        KeyCode::KeyD.press(app.world_mut());
        app.update();

        let velocity = app.world().get::<Velocity>(entity).unwrap();
        assert_eq!(velocity.0.x, 250.0);
    }

    #[test]
    fn player_input_uses_scripted_jump_force() {
        let mut app = player_test_app();
        let entity = app
            .world_mut()
            .spawn((
                Player,
                Velocity::default(),
                Grounded(true),
                ScriptedTuning {
                    move_speed: 200.0,
                    jump_force: 650.0,
                    gravity: 980.0,
                },
                player_input_map(),
            ))
            .id();

        KeyCode::Space.press(app.world_mut());
        app.update();

        let velocity = app.world().get::<Velocity>(entity).unwrap();
        assert_eq!(velocity.0.y, 650.0);
    }

    #[test]
    fn apply_gravity_uses_scripted_gravity() {
        use bevy::time::TimeUpdateStrategy;
        use std::time::Duration;

        // Bevy's per-frame delta under a manual time strategy isn't a documented
        // exact value in tests (the first frame is always zero to establish a
        // baseline). Instead of asserting one absolute number, run the system
        // twice with different ScriptedTuning.gravity values under identical
        // timing and assert the velocity change scales with gravity — this
        // proves apply_gravity reads the component without depending on Bevy's
        // internal time-harness behavior.
        fn run_with_gravity(gravity: f32) -> f32 {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                0.5,
            )));
            let entity = app
                .world_mut()
                .spawn((
                    Player,
                    Velocity::default(),
                    ScriptedTuning {
                        move_speed: 200.0,
                        jump_force: 500.0,
                        gravity,
                    },
                ))
                .id();
            app.add_systems(Update, apply_gravity);

            app.update();
            app.update();

            app.world().get::<Velocity>(entity).unwrap().0.y
        }

        let low = run_with_gravity(500.0);
        let high = run_with_gravity(1000.0);

        assert!(low < 0.0, "gravity should pull velocity negative, got {low}");
        assert!(
            (high - low * 2.0).abs() < 0.01,
            "doubling ScriptedTuning.gravity should double the velocity change: low={low} high={high}"
        );
    }
}