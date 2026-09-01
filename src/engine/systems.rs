use crate::engine::components::*;
use crate::engine::input::PlayerAction;
use crate::engine::scripting::ScriptedTuning;
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

pub fn player_input(
    tuning: Res<ScriptedTuning>,
    mut query: Query<(&ActionState<PlayerAction>, &mut Velocity, &Grounded), With<Player>>,
) {
    let Ok((action_state, mut velocity, grounded)) = query.single_mut() else {
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
    tuning: Res<ScriptedTuning>,
    mut query: Query<&mut Velocity, With<Player>>,
) {
    for mut velocity in &mut query {
        velocity.0.y -= tuning.gravity * time.delta_secs();
    }
}

pub fn apply_velocity(time: Res<Time>, mut query: Query<(&mut Transform, &Velocity)>) {
    for (mut transform, velocity) in &mut query {
        transform.translation.x += velocity.0.x * time.delta_secs();
        transform.translation.y += velocity.0.y * time.delta_secs();
    }
}

pub fn ground_detection(
    mut player_query: Query<(&Transform, &Sprite, &mut Grounded, &mut Velocity), With<Player>>,
    platform_query: Query<(&Transform, &Sprite), With<Platform>>,
) {
    let Ok((player_transform, player_sprite, mut grounded, mut velocity)) =
        player_query.single_mut()
    else {
        return;
    };

    let player_size = player_sprite.custom_size.unwrap_or(Vec2::ZERO);
    let player_half = player_size * 0.5;
    let player_min = Vec2::new(
        player_transform.translation.x - player_half.x,
        player_transform.translation.y - player_half.y,
    );
    let player_max = Vec2::new(
        player_transform.translation.x + player_half.x,
        player_transform.translation.y + player_half.y,
    );

    let mut on_ground = false;
    const GROUND_MARGIN: f32 = 2.0;

    for (plat_transform, plat_sprite) in &platform_query {
        let plat_size = plat_sprite.custom_size.unwrap_or(Vec2::ZERO);
        let plat_half = plat_size * 0.5;
        let plat_min = Vec2::new(
            plat_transform.translation.x - plat_half.x,
            plat_transform.translation.y - plat_half.y,
        );
        let plat_max = Vec2::new(
            plat_transform.translation.x + plat_half.x,
            plat_transform.translation.y + plat_half.y,
        );

        let   overlap_x = player_max.x > plat_min.x && player_min.x < plat_max.x;
        let  player_bottom_in_plat =
             player_min.y <= plat_max.y + GROUND_MARGIN && player_min.y >= plat_min.y;
        let player_falling = velocity.0.y <= 0.0;

        if overlap_x && player_bottom_in_plat && player_falling {
            on_ground = true;
            velocity.0.y = 0.0;
        }
    }

    grounded.0 = on_ground;
}

pub fn camera_follow(
    mut camera_query: Query<&mut Transform, (With<Camera2d>, Without<Player>)>,
    player_query: Query<&Transform, With<Player>>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };

    camera_transform.translation.x = player_transform.translation.x;
    camera_transform.translation.y = player_transform.translation.y;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::input::player_input_map;
    use crate::engine::scripting::ScriptedTuning;
    use bevy::input::InputPlugin;

    fn player_test_app(tuning: ScriptedTuning) -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputPlugin,
            InputManagerPlugin::<PlayerAction>::default(),
        ));
        app.insert_resource(tuning);
        app.add_systems(Update, player_input);
        app
    }

    #[test]
    fn player_input_uses_scripted_move_speed() {
        let mut app = player_test_app(ScriptedTuning {
            move_speed: 250.0,
            jump_force: 500.0,
            gravity: 980.0,
        });
        let entity = app
            .world_mut()
            .spawn((
                Player,
                Velocity::default(),
                Grounded(false),
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
        let mut app = player_test_app(ScriptedTuning {
            move_speed: 200.0,
            jump_force: 650.0,
            gravity: 980.0,
        });
        let entity = app
            .world_mut()
            .spawn((
                Player,
                Velocity::default(),
                Grounded(true),
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
        // proves apply_gravity reads the resource without depending on Bevy's
        // internal time-harness behavior.
        fn run_with_gravity(gravity: f32) -> f32 {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                0.5,
            )));
            app.insert_resource(ScriptedTuning {
                move_speed: 200.0,
                jump_force: 500.0,
                gravity,
            });
            let entity = app.world_mut().spawn((Player, Velocity::default())).id();
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