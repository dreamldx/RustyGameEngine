use crate::engine::components::*;
use crate::engine::input::player_input_map;
use bevy::prelude::*;

const PLAYER_SIZE: Vec2 = Vec2::new(40.0, 48.0);
const PLAYER_COLOR: Color = Color::srgb(0.2, 0.4, 0.9);

pub fn spawn_player(mut commands: Commands, spawn: Res<PlayerSpawn>) {
    commands.spawn((
        Sprite {
            color: PLAYER_COLOR,
            custom_size: Some(PLAYER_SIZE),
            ..default()
        },
        Transform::from_xyz(spawn.0.x, spawn.0.y, 1.0),
        Player,
        Velocity::default(),
        Grounded::default(),
        player_input_map(),
    ));
}

pub fn spawn_camera(mut commands: Commands) {
    commands.spawn((Camera2d, Transform::from_xyz(0.0, 0.0, 10.0)));
}