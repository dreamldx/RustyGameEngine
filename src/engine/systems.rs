use crate::engine::ReadyToPlay;
use crate::engine::components::*;
use crate::engine::debug_ui::{self, ReloadLevelRequested};
use crate::engine::input::{self, PlayerAction};
use crate::engine::level::{self, LEVEL_MAX_X, LEVEL_MIN_X};
use crate::engine::scripting::{self, ScriptedTuning};
use crate::engine::{camera, player};
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

pub struct GameplaySystemsPlugin;

impl Plugin for GameplaySystemsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReloadLevelRequested>()
            .init_resource::<ReadyToPlay>()
            .init_resource::<input::PendingPlayerInputMap>()

            .add_plugins((
                player::PlayerPlugin,
                camera::CameraPlugin,
                level::LevelLoadPlugin,
                debug_ui::DebugUiPlugin,
            ))
            .add_systems(
                Startup,
                (scripting::load_scripts, level::load_level_scripts).chain(),
            )
            .add_systems(
                Update,
                (
                    player_input,
                    apply_gravity,
                    ground_detection,
                    apply_velocity,
                    clamp_player_bounds,
                ).chain(),
            )
            .add_systems(Update, (check_fall, input::apply_player_input_map));
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

pub fn apply_velocity(time: Res<Time>, mut query: Query<(&mut Transform, &Velocity)>) {
    for (mut transform, velocity) in &mut query {
        transform.translation.x += velocity.0.x * time.delta_secs();
        transform.translation.y += velocity.0.y * time.delta_secs();
    }
}

fn world_vertices(local_verts: &[Vec2], transform: &Transform) -> Vec<Vec2> {
    local_verts
        .iter()
        .map(|v| {
            let w = transform.transform_point(Vec3::new(v.x, v.y, 0.0));
            Vec2::new(w.x, w.y)
        })
        .collect()
}

fn project(vertices: &[Vec2], axis: Vec2) -> (f32, f32) {
    let mut min = f32::MAX;
    let mut max = f32::MIN;
    for v in vertices {
        let dot = v.dot(axis);
        min = min.min(dot);
        max = max.max(dot);
    }
    (min, max)
}

fn axis_overlap(min1: f32, max1: f32, min2: f32, max2: f32) -> Option<f32> {
    if max1 < min2 || max2 < min1 {
        return None;
    }
    Some(f32::min(max1 - min2, max2 - min1))
}

fn edge_normals(vertices: &[Vec2]) -> Vec<Vec2> {
    let n = vertices.len();
    let mut axes = Vec::with_capacity(n);
    for i in 0..n {
        let j = (i + 1) % n;
        let edge = vertices[j] - vertices[i];
        let normal = Vec2::new(-edge.y, edge.x);
        let len = normal.length();
        if len > 0.0001 {
            axes.push(normal / len);
        }
    }
    axes
}

fn sat_collision(verts_a: &[Vec2], verts_b: &[Vec2]) -> Option<(Vec2, f32)> {
    let mut min_overlap = f32::MAX;
    let mut mtv = Vec2::ZERO;

    let axes_a = edge_normals(verts_a);
    let axes_b = edge_normals(verts_b);

    for axis in axes_a.iter().chain(axes_b.iter()) {
        let (min_a, max_a) = project(verts_a, *axis);
        let (min_b, max_b) = project(verts_b, *axis);
        if let Some(o) = axis_overlap(min_a, max_a, min_b, max_b) {
            if o < min_overlap {
                min_overlap = o;
                mtv = *axis;
            }
        } else {
            return None;
        }
    }

    if min_overlap < f32::MAX {
        Some((mtv, min_overlap))
    } else {
        None
    }
}

pub fn ground_detection(
    mut player_query: Query<
        (&Transform, &Collider, &mut Grounded, &mut Velocity),
        With<Player>,
    >,
    platform_query: Query<(&Transform, &Collider), With<Platform>>,
) {
    let Ok((player_transform, player_collider, mut grounded, mut velocity)) =
        player_query.single_mut()
    else {
        return;
    };

    let player_verts = world_vertices(&player_collider.0, player_transform);

    let mut on_ground = false;

    for (plat_transform, plat_collider) in &platform_query {
        let plat_verts = world_vertices(&plat_collider.0, plat_transform);
        let player_falling = velocity.0.y <= 0.0;

        if let Some((mtv, _)) = sat_collision(&player_verts, &plat_verts) {
            let player_center = player_transform.translation.truncate();
            let plat_center = plat_transform.translation.truncate();
            let to_player = player_center - plat_center;
            let directed_mtv = if mtv.dot(to_player) < 0.0 { -mtv } else { mtv };

            if directed_mtv.y > 0.0 && player_falling {
                on_ground = true;
                velocity.0.y = 0.0;
            }
        }
    }

    grounded.0 = on_ground;
}

/// Keeps the player within the level's horizontal bounds — trying to walk
/// past either edge just stops movement there instead of leaving the level.
pub fn clamp_player_bounds(mut query: Query<(&mut Transform, &mut Velocity), With<Player>>) {
    let Ok((mut transform, mut velocity)) = query.single_mut() else {
        return;
    };
    if transform.translation.x < LEVEL_MIN_X {
        transform.translation.x = LEVEL_MIN_X;
        velocity.0.x = velocity.0.x.max(0.0);
    } else if transform.translation.x > LEVEL_MAX_X {
        transform.translation.x = LEVEL_MAX_X;
        velocity.0.x = velocity.0.x.min(0.0);
    }
}

const FALL_DEATH_Y: f32 = -1000.0;

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