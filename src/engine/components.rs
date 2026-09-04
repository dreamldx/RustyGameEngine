use bevy::prelude::*;

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct Platform;

#[derive(Component, Default)]
pub struct Velocity(pub Vec2);

#[derive(Component)]
pub struct Grounded(pub bool);

#[derive(Component)]
pub struct Collider(pub Vec<Vec2>);

#[derive(Resource)]
pub struct PlayerSpawn(pub Vec2);

impl Default for Grounded {
    fn default() -> Self {
        Self(false)
    }
}