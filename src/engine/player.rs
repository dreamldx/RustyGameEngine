use crate::engine::ReadyToPlay;
use crate::engine::camera;
use crate::engine::components::*;
use crate::engine::input::player_input_map;
use crate::engine::scripting::ScriptedTuning;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

const PLAYER_SIZE: Vec2 = Vec2::new(40.0, 48.0);
const PLAYER_COLOR: Color = Color::srgb(0.2, 0.4, 0.9);

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_player);
    }
}

/// Spawns the player once level `load()` has populated `PlayerSpawn`, and
/// reframes the camera (spawned separately at Startup, see `camera.rs`) so
/// the player sits at the lower quarter of the view — this runs on every
/// "Reload Level" respawn too, not just the first time, so the camera
/// doesn't stay wherever it drifted to during the previous attempt. Runs
/// with direct `&mut World` access for the same reason as
/// `debug_ui::apply_level_reload`: no deferred-command ordering to
/// coordinate.
pub fn spawn_player(world: &mut World) {
    let should_spawn = {
        let mut ready = world.resource_mut::<ReadyToPlay>();
        let should_spawn = ready.0;
        ready.0 = false;
        should_spawn
    };
    if !should_spawn {
        return;
    }

    let spawn = world.resource::<PlayerSpawn>().0;
    world.spawn((
        Sprite {
            color: PLAYER_COLOR,
            custom_size: Some(PLAYER_SIZE),
            ..default()
        },
        Transform::from_xyz(spawn.x, spawn.y, 1.0),
        Collider(vec![
            Vec2::new(-PLAYER_SIZE.x / 2.0, -PLAYER_SIZE.y / 2.0),
            Vec2::new(PLAYER_SIZE.x / 2.0, -PLAYER_SIZE.y / 2.0),
            Vec2::new(PLAYER_SIZE.x / 2.0, PLAYER_SIZE.y / 2.0),
            Vec2::new(-PLAYER_SIZE.x / 2.0, PLAYER_SIZE.y / 2.0),
        ]),
        Player,
        Velocity::default(),
        Grounded::default(),
        ScriptedTuning::default(),
        player_input_map(),
    ));

    // Reframe the camera so the player sits at the lower quarter of the
    // view. The camera normally already exists by now (CameraPlugin spawns
    // it at Startup, well before this ever runs); the fallback spawn below
    // is defensive only, for the unlikely case it's somehow missing.
    let mut camera_query = world.query_filtered::<(&mut Transform, &Projection), With<Camera2d>>();
    if let Ok((mut camera_transform, projection)) = camera_query.single_mut(world) {
        let half_height = match projection {
            Projection::Orthographic(ortho) => ortho.area.height() / 2.0,
            _ => 0.0,
        };
        // Matches camera_follow's dead-zone half-width — the two must
        // agree, or first-spawn and reload frame the player differently.
        camera_transform.translation.y = spawn.y + half_height / 2.0;
    } else if let Err(e) = world.run_system_once(camera::spawn_camera) {
        error!("Failed to spawn camera: {e}");
    }
}