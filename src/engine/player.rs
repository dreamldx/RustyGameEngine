use crate::engine::ReadyToPlay;
use crate::engine::camera;
use crate::engine::components::*;
use crate::engine::input::player_input_map;
use crate::engine::scripting::ScriptedTuning;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy_mod_scripting::prelude::*;
use bevy_rapier2d::prelude::{Collider, KinematicCharacterController, RigidBody};

const PLAYER_SIZE: Vec2 = Vec2::new(40.0, 48.0);
const PLAYER_COLOR: Color = Color::srgb(0.2, 0.4, 0.9);

callback_labels!(OnSceneLoaded => "on_scene_loaded");

/// Set by `spawn_player` right after the player entity is actually spawned
/// (initial load and every "Reload Level"); consumed by `broadcast_scene_loaded`,
/// which fires `on_scene_loaded` at every loaded script. This gives scripts an
/// explicit, correctly-timed hook for anything that needs the player to
/// exist (tuning, input bindings) — unlike BMS's automatic `on_script_loaded`,
/// which fires as soon as a script itself loads, before the player is spawned.
#[derive(Resource, Default)]
pub struct SceneLoadedRequested(pub bool);

/// Fires `on_scene_loaded` on every currently-loaded script entity once
/// `SceneLoadedRequested` is set. A script that doesn't define
/// `on_scene_loaded` is unaffected (same no-op behavior BMS already uses for
/// scripts that don't define `on_script_loaded`).
pub fn broadcast_scene_loaded(
    mut requested: ResMut<SceneLoadedRequested>,
    scripts: Query<(Entity, &ScriptComponent)>,
    mut callbacks: MessageWriter<ScriptCallbackEvent>,
) {
    if !requested.0 {
        return;
    }
    requested.0 = false;
    for (entity, script) in &scripts {
        let Some(handle) = script.0.first() else {
            continue;
        };
        callbacks.write(ScriptCallbackEvent::new_for_script_entity(
            OnSceneLoaded,
            vec![],
            handle.clone(),
            entity,
        ));
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneLoadedRequested>().add_systems(
            Update,
            (
                spawn_player,
                broadcast_scene_loaded,
                event_handler::<OnSceneLoaded, LuaScriptingPlugin>,
            )
                .chain(),
        );
    }
}

/// Spawns the player once level `on_level_load()` has populated `PlayerSpawn`, and
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
    world.resource_mut::<SceneLoadedRequested>().0 = true;
    world.spawn((
        Sprite {
            color: PLAYER_COLOR,
            custom_size: Some(PLAYER_SIZE),
            ..default()
        },
        Transform::from_xyz(spawn.x, spawn.y, 1.0),
        RigidBody::KinematicPositionBased,
        Collider::cuboid(PLAYER_SIZE.x / 2.0, PLAYER_SIZE.y / 2.0),
        KinematicCharacterController::default(),
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