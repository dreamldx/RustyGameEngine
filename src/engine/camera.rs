use crate::engine::components::Player;
use crate::engine::level::LevelBounds;
use bevy::prelude::*;
use bevy_rapier2d::prelude::PhysicsSet;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        // Spawned at Startup — not once we actually know PlayerSpawn — so
        // that bevy_render's camera_system gets to compute its real
        // Projection area in the one-time PostStartup pass, before the
        // first Update tick ever runs. A camera created later, during
        // Update (e.g. only once the level's on_level_load() responds), misses that
        // PostStartup pass entirely and sits on a placeholder Projection
        // area for a full frame, which player::spawn_player's own
        // reframing logic can't tell apart from a real (tiny) viewport.
        // The starting Transform here is a throwaway; spawn_player resets
        // it correctly once PlayerSpawn is actually known.
        app.add_systems(Startup, spawn_camera)
            // The player's Transform isn't resolved for this frame until
            // Rapier's PostUpdate step processes `move_player_kcc`'s
            // requested translation (see systems.rs) — reading it any
            // earlier (e.g. in Update) would follow last frame's stale
            // position instead of where the player actually ends up.
            .add_systems(PostUpdate, camera_follow.after(PhysicsSet::Writeback));
    }
}

pub fn spawn_camera(mut commands: Commands) {
    commands.spawn((Camera2d, Transform::from_xyz(0.0, 0.0, 10.0)));
}

/// Follows the player horizontally/vertically, but clamps by the camera's
/// own viewport edges rather than its center — otherwise the center could
/// sit inside `[LevelBounds::min_x, LevelBounds::max_x]` while half the
/// viewport still shows empty space past the level's edge.
pub fn camera_follow(
    mut camera_query: Query<(&mut Transform, &Projection), (With<Camera2d>, Without<Player>)>,
    player_query: Query<&Transform, With<Player>>,
    bounds: Res<LevelBounds>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let Ok((mut camera_transform, projection)) = camera_query.single_mut() else {
        return;
    };

    let (half_width, half_height) = match projection {
        Projection::Orthographic(ortho) => (ortho.area.width() / 2.0, ortho.area.height() / 2.0),
        _ => (0.0, 0.0),
    };
    // A freshly spawned camera's Projection area is still the engine's
    // placeholder Rect (width/height 2.0, i.e. half-extents of 1.0) for the
    // rest of the frame it was created on — Bevy only computes the real
    // viewport-sized area in a later schedule. Acting on that placeholder
    // here would treat the player as wildly outside the dead zone and snap
    // the camera to a bogus position that then never self-corrects (the
    // real half_height only shows up starting next frame, by which point
    // the wrong position already looks "current"). Just wait a frame.
    if half_width <= 1.0 || half_height <= 1.0 {
        return;
    }
    let min_x = bounds.min_x + half_width;
    let max_x = (bounds.max_x - half_width).max(min_x);
    camera_transform.translation.x = player_transform.translation.x.clamp(min_x, max_x);

    // Vertical dead zone: the camera only moves once the player leaves the
    // middle half of the viewport (i.e. crosses more than a quarter of the
    // viewport height above/below the camera's current center), and just
    // enough to keep them pinned at whichever edge of that band they
    // crossed — it otherwise stays exactly where it was last frame.
    let half_dead_zone = half_height / 2.0;
    let lower_bound = camera_transform.translation.y - half_dead_zone;
    let upper_bound = camera_transform.translation.y + half_dead_zone;
    if player_transform.translation.y > upper_bound {
        camera_transform.translation.y = player_transform.translation.y - half_dead_zone;
    } else if player_transform.translation.y < lower_bound {
        camera_transform.translation.y = player_transform.translation.y + half_dead_zone;
    }
}
