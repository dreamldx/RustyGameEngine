use crate::engine::ReadyToPlay;
use crate::engine::components::{Platform, Player, PlayerSpawn};
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_mod_scripting::prelude::*;
use bevy_mod_scripting_bindings::{FunctionCallContext, InteropError, ScriptValue};
use bevy_mod_scripting_core::event::ScriptCallbackResponseEvent;
use bevy_rapier2d::prelude::{Collider, RigidBody};
use std::fs;
use std::path::Path;

/// Horizontal bounds of the current level. Defaults to a wide fallback range
/// for a level script that never calls `world.set_level_bounds(...)` (or a
/// yscn scene with no `LevelBounds` entity). The player is kept inside this
/// range by the invisible walls `sync_level_bounds_walls` spawns; the camera
/// clamps to it separately (see `camera::camera_follow`).
#[derive(Resource, Clone, Copy)]
pub struct LevelBounds {
    pub min_x: f32,
    pub max_x: f32,
}

impl Default for LevelBounds {
    fn default() -> Self {
        Self { min_x: 0.0, max_x: 10800.0 }
    }
}

/// Whether the current level's `load()` has explicitly called
/// `world.set_level_bounds(...)`. Reset to `false` each time a level load is
/// requested (`request_level_load`); if still `false` once `load()`
/// finishes, `handle_level_load_response` falls back to `0..ObservedMaxX`
/// instead of leaving the *previous* level's `LevelBounds` in place.
#[derive(Resource, Default)]
pub struct LevelBoundsExplicit(pub bool);

/// Running maximum world-space x-coordinate across every platform spawned
/// so far for the current level (updated by `spawn_platform_entity`). Reset
/// to `0.0` each time a level load is requested. Used as the auto-computed
/// `LevelBounds::max_x` fallback — see `LevelBoundsExplicit`.
#[derive(Resource, Default)]
pub struct ObservedMaxX(pub f32);

/// Marks the two invisible boundary-wall entities `sync_level_bounds_walls`
/// spawns, so it can despawn and respawn them when `LevelBounds` changes.
/// Deliberately not `Platform`, so `reload_level`'s despawn (which only
/// targets `Platform`/`Player`) leaves them alone in between.
#[derive(Component)]
pub struct BoundaryWall;

/// Half-extents of each boundary wall. The height just needs to comfortably
/// exceed any player position a level could produce — it doesn't need to
/// match the level's actual vertical extent, since the walls only exist to
/// block horizontal movement past `LevelBounds::min_x`/`max_x`.
const BOUNDARY_WALL_HALF_WIDTH: f32 = 10.0;
const BOUNDARY_WALL_HALF_HEIGHT: f32 = 100_000.0;

/// Respawns the two static invisible boundary walls whenever `LevelBounds`
/// changes (initial default on startup, or a level script/yscn scene
/// setting its own range during `load()`). Rapier's own character-controller
/// sweep then enforces the range the same way it enforces collision with any
/// platform, instead of a separate manual clamp in `systems::move_player_kcc`.
pub fn sync_level_bounds_walls(
    mut commands: Commands,
    bounds: Res<LevelBounds>,
    walls: Query<Entity, With<BoundaryWall>>,
) {
    if !bounds.is_changed() {
        return;
    }
    for entity in &walls {
        commands.entity(entity).despawn();
    }
    let half_width = BOUNDARY_WALL_HALF_WIDTH;
    let half_height = BOUNDARY_WALL_HALF_HEIGHT;
    for wall_center_x in [bounds.min_x - half_width, bounds.max_x + half_width] {
        commands.spawn((
            Transform::from_xyz(wall_center_x, 0.0, 0.0),
            RigidBody::Fixed,
            Collider::cuboid(half_width, half_height),
            BoundaryWall,
        ));
    }
}

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

    let Some(collider) = Collider::convex_hull(&centered_verts) else {
        error!("spawn_platform_entity: points don't form a valid convex hull");
        return;
    };

    let platform_max_x = centered_verts
        .iter()
        .map(|v| v.x + pos.x)
        .fold(f32::MIN, f32::max);
    let mut observed_max_x = world.resource_mut::<ObservedMaxX>();
    observed_max_x.0 = observed_max_x.0.max(platform_max_x);

    let mesh_handle = world.resource_mut::<Assets<Mesh>>().add(mesh);
    let material_handle = world
        .resource_mut::<Assets<ColorMaterial>>()
        .add(ColorMaterial::from(Color::srgba(color[0], color[1], color[2], color[3])));

    world.spawn((
        Mesh2d(mesh_handle),
        MeshMaterial2d(material_handle),
        Transform::from_translation(pos),
        RigidBody::Fixed,
        collider,
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
            .init_resource::<LevelBounds>()
            .init_resource::<LevelBoundsExplicit>()
            .init_resource::<ObservedMaxX>()
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
                    sync_level_bounds_walls,
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

    /// Called from a level script's `load()` as
    /// `world.set_level_bounds(min_x, max_x)`. Optional — a level that never
    /// calls this gets `0..ObservedMaxX` instead (see `LevelBoundsExplicit`).
    /// Triggers `sync_level_bounds_walls` to respawn the boundary walls at
    /// the new range.
    pub fn set_level_bounds(
        context: FunctionCallContext,
        min_x: f32,
        max_x: f32,
    ) -> Result<(), InteropError> {
        let world = context.world()?;
        world.with_world_mut_access(|world| {
            world.insert_resource(LevelBounds { min_x, max_x });
            world.resource_mut::<LevelBoundsExplicit>().0 = true;
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
    mut bounds_explicit: ResMut<LevelBoundsExplicit>,
    mut observed_max_x: ResMut<ObservedMaxX>,
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

    bounds_explicit.0 = false;
    observed_max_x.0 = 0.0;
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
    bounds_explicit: Res<LevelBoundsExplicit>,
    observed_max_x: Res<ObservedMaxX>,
    mut bounds: ResMut<LevelBounds>,
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
        if !bounds_explicit.0 {
            *bounds = LevelBounds { min_x: 0.0, max_x: observed_max_x.0 };
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
