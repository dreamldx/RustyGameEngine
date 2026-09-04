use crate::engine::ReadyToPlay;
use crate::engine::components::{Collider, Platform, Player, PlayerSpawn};
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_mod_scripting::prelude::*;
use bevy_mod_scripting_bindings::{FunctionCallContext, InteropError, ScriptValue};
use bevy_mod_scripting_core::event::ScriptCallbackResponseEvent;
use std::fs;
use std::path::Path;

/// Horizontal bounds of the level. The player and camera are both clamped
/// to this range (see `systems::clamp_player_bounds`/`systems::camera_follow`).
pub const LEVEL_MIN_X: f32 = 0.0;
pub const LEVEL_MAX_X: f32 = 10800.0;

fn compute_bounds(vertices: &[Vec2]) -> (Vec2, Vec2) {
    let min_x = vertices.iter().map(|v| v.x).reduce(f32::min).unwrap_or(0.0);
    let min_y = vertices.iter().map(|v| v.y).reduce(f32::min).unwrap_or(0.0);
    let max_x = vertices.iter().map(|v| v.x).reduce(f32::max).unwrap_or(0.0);
    let max_y = vertices.iter().map(|v| v.y).reduce(f32::max).unwrap_or(0.0);
    let center = Vec2::new((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
    let size = Vec2::new(max_x - min_x, max_y - min_y);
    (center, size)
}

fn triangulate_convex(vertices: &[Vec2]) -> Vec<u32> {
    let mut indices = Vec::new();
    for i in 1..=(vertices.len() - 2) {
        indices.extend_from_slice(&[0, i as u32, i as u32 + 1]);
    }
    indices
}

/// Builds and spawns a single convex `Platform` entity from a position and a
/// list of points (relative to that position). Shared by the Lua-driven
/// `world.spawn_platform(...)` binding below — this is the only place that
/// turns level geometry into an actual ECS entity + mesh.
pub fn spawn_platform_entity(
    world: &mut World,
    position: Vec2,
    raw_verts: Vec<Vec2>,
    color: [f32; 4],
) {
    if raw_verts.len() < 3 {
        error!("spawn_platform_entity: need at least 3 points, got {}", raw_verts.len());
        return;
    }

    let (center, _size) = compute_bounds(&raw_verts);
    let centered_verts: Vec<Vec2> = raw_verts.iter().map(|v| *v - center).collect();
    let pos = Vec3::new(position.x + center.x, position.y + center.y, 0.0);

    let positions: Vec<Vec3> = centered_verts
        .iter()
        .map(|v| Vec3::new(v.x, v.y, 0.0))
        .collect();
    let indices = triangulate_convex(&centered_verts);
    let normals: Vec<Vec3> = std::iter::repeat_n(Vec3::Z, positions.len()).collect();
    let uvs: Vec<Vec2> = std::iter::repeat_n(Vec2::ZERO, positions.len()).collect();

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_indices(Indices::U32(indices));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);

    let mesh_handle = world.resource_mut::<Assets<Mesh>>().add(mesh);
    let material_handle = world
        .resource_mut::<Assets<ColorMaterial>>()
        .add(ColorMaterial::from(Color::srgba(color[0], color[1], color[2], color[3])));

    world.spawn((
        Mesh2d(mesh_handle),
        MeshMaterial2d(material_handle),
        Transform::from_translation(pos),
        Collider(centered_verts),
        Platform,
    ));
}

/// Maps a level name to its script entity, populated by `load_level_scripts`
/// (from the filename of `assets/scripts/levels/<name>.lua`). The mapping
/// happens entirely Rust-side rather than via a script-side registration
/// call, because the bindings API in this bevy_mod_scripting version has no
/// way for a bound function to know which script called it
/// (`ThreadScriptContext::attachment` is commented out upstream) — Rust just
/// uses the filename it already has when it spawns the script entity.
#[derive(Resource, Default)]
pub struct LevelRegistry(pub HashMap<String, Entity>);

/// Set to request that a named level's `load()` function be called. Cleared
/// once the request has been sent (see `PendingLevelLoad` for the in-flight
/// state after that).
#[derive(Resource, Default)]
pub struct LevelLoadRequested(pub Option<String>);

/// The level name currently waiting on a `load()` callback response.
#[derive(Resource, Default)]
pub struct PendingLevelLoad(pub Option<String>);

callback_labels!(OnLoadLevel => "load");

pub struct LevelLoadPlugin;

impl Plugin for LevelLoadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LevelRegistry>()
            .init_resource::<PendingLevelLoad>()
            .insert_resource(LevelLoadRequested(Some("main".to_string())))
            .add_systems(
                Update,
                (
                    request_level_load,
                    handle_level_load_response,
                    // Actually dispatches ScriptCallbackEvent -> the script's
                    // Lua `load` function. Without this, request_level_load's
                    // event just sits in the message queue forever.
                    event_handler::<OnLoadLevel, LuaScriptingPlugin>,
                ),
            );
    }
}

#[script_bindings(remote, unregistered)]
impl World {
    /// Called from a level script's `load()` (e.g. `assets/scripts/levels/main.lua`)
    /// as `world.spawn_platform(x, y, points, color)`. `points` is a list of
    /// `{x=.., y=..}` tables (relative to x/y), `color` is a `{r,g,b,a}` list.
    pub fn spawn_platform(
        context: FunctionCallContext,
        x: f32,
        y: f32,
        points: ScriptValue,
        color: ScriptValue,
    ) -> Result<(), InteropError> {
        let world = context.world()?;
        world.with_world_mut_access(|world| {
            let Some(verts) = parse_points(&points) else {
                warn!("spawn_platform: points must be a list of {{x=,y=}} tables");
                return;
            };
            let color = parse_color(&color).unwrap_or([0.3, 0.6, 0.3, 1.0]);
            spawn_platform_entity(world, Vec2::new(x, y), verts, color);
        })?;
        Ok(())
    }

    /// Called from a level script's `load()` as `world.set_player_spawn(x, y)`.
    pub fn set_player_spawn(context: FunctionCallContext, x: f32, y: f32) -> Result<(), InteropError> {
        let world = context.world()?;
        world.with_world_mut_access(|world| {
            world.insert_resource(PlayerSpawn(Vec2::new(x, y)));
        })?;
        Ok(())
    }
}

fn script_value_to_f32(value: &ScriptValue) -> Option<f32> {
    match value {
        ScriptValue::Float(f) => Some(*f as f32),
        ScriptValue::Integer(i) => Some(*i as f32),
        _ => None,
    }
}

fn parse_points(value: &ScriptValue) -> Option<Vec<Vec2>> {
    let ScriptValue::List(list) = value else {
        return None;
    };
    list.iter()
        .map(|v| {
            let ScriptValue::Map(m) = v else { return None };
            let x = script_value_to_f32(m.get("x")?)?;
            let y = script_value_to_f32(m.get("y")?)?;
            Some(Vec2::new(x, y))
        })
        .collect()
}

fn parse_color(value: &ScriptValue) -> Option<[f32; 4]> {
    let ScriptValue::List(list) = value else {
        return None;
    };
    if list.len() != 4 {
        return None;
    }
    let mut out = [0.0; 4];
    for (slot, v) in out.iter_mut().zip(list.iter()) {
        *slot = script_value_to_f32(v)?;
    }
    Some(out)
}

/// Scans `assets/scripts/levels/` for `.lua` files. Each file's name (minus
/// `.lua`) is recorded in `LevelRegistry` as its level name, immediately
/// (registration doesn't need to wait for the script to finish loading —
/// only actually *calling* `load()` does, which `request_level_load`
/// handles separately).
pub fn load_level_scripts(
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    mut registry: ResMut<LevelRegistry>,
) {
    let dir = Path::new("assets/scripts/levels");
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            error!("Failed to read levels directory {}: {}", dir.display(), e);
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
        let Some(name) = file_name.strip_suffix(".lua") else {
            continue;
        };

        let handle = asset_server.load::<ScriptAsset>(format!("scripts/levels/{file_name}"));
        let entity = commands.spawn(ScriptComponent(vec![handle])).id();
        registry.0.insert(name.to_string(), entity);
    }
}

/// Sends the `load` callback for the requested level, once. Doesn't retry —
/// if the script isn't loaded yet when this fires, the event is dropped by
/// bevy_mod_scripting's own dispatch (no context to call into), same as any
/// callback sent for an unloaded script.
fn request_level_load(
    mut requested: ResMut<LevelLoadRequested>,
    mut pending: ResMut<PendingLevelLoad>,
    registry: Res<LevelRegistry>,
    scripts: Query<&ScriptComponent>,
    mut callbacks: MessageWriter<ScriptCallbackEvent>,
) {
    let Some(name) = requested.0.clone() else {
        return;
    };
    if pending.0.is_some() {
        return;
    }
    let Some(&entity) = registry.0.get(&name) else {
        return;
    };
    let Ok(script) = scripts.get(entity) else {
        return;
    };
    let Some(handle) = script.0.first() else {
        return;
    };

    callbacks.write(
        ScriptCallbackEvent::new_for_script_entity(OnLoadLevel, vec![], handle.clone(), entity)
            .with_response(),
    );
    pending.0 = Some(name);
    requested.0 = None;
}

fn handle_level_load_response(
    mut pending: ResMut<PendingLevelLoad>,
    mut responses: MessageReader<ScriptCallbackResponseEvent>,
    mut ready_to_play: ResMut<ReadyToPlay>,
) {
    let Some(name) = pending.0.clone() else {
        return;
    };
    for response in responses.read() {
        if response.label != OnLoadLevel.into() {
            continue;
        }
        if let Err(e) = &response.response {
            error!("Level '{name}' load() failed: {e}");
        }
        pending.0 = None;
        ready_to_play.0 = true;
        break;
    }
}

/// Despawns all `Platform`/`Player` entities and re-requests the given
/// level's `load()`, for the "Reload Level" button.
pub fn reload_level(world: &mut World, name: &str) {
    let mut platform_query = world.query_filtered::<Entity, With<Platform>>();
    let platforms: Vec<Entity> = platform_query.iter(world).collect();
    let mut player_query = world.query_filtered::<Entity, With<Player>>();
    let players: Vec<Entity> = player_query.iter(world).collect();
    for entity in platforms.into_iter().chain(players) {
        world.despawn(entity);
    }
    world.resource_mut::<LevelLoadRequested>().0 = Some(name.to_string());
}
