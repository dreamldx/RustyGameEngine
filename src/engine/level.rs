use crate::engine::components::{Platform, PlayerSpawn};
use bevy::prelude::*;

const TILE_SIZE: f32 = 32.0;
const PLATFORM_COLOR: Color = Color::srgb(0.3, 0.6, 0.3);

pub fn spawn_level(mut commands: Commands) {
    let text = std::fs::read_to_string("assets/levels/level1.txt").unwrap_or_else(|e| {
        error!("Failed to read level file: {}", e);
        String::new()
    });

    let grid: Vec<&str> = text.lines().collect();
    if grid.is_empty() {
        return;
    }

    let rows = grid.len() as f32;
    let cols = grid.iter().map(|l| l.len()).max().unwrap_or(0) as f32;

    let offset_x = -(cols * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_y = (rows * TILE_SIZE) / 2.0 - TILE_SIZE / 2.0;

    let mut player_spawn = Vec2::ZERO;

    for (row, line) in grid.iter().enumerate() {
        let mut col = 0;
        while col < line.len() {
            let ch = line.as_bytes().get(col).map(|&b| b as char).unwrap_or('.');
            match ch {
                '#' => {
                    let start = col;
                    while col < line.len()
                        && line.as_bytes().get(col).map_or(false, |&b| b as char == '#')
                    {
                        col += 1;
                    }
                    let count = (col - start) as f32;
                    let world_x = offset_x + (start as f32 + (count - 1.0) / 2.0) * TILE_SIZE;
                    let world_y = offset_y - row as f32 * TILE_SIZE;
                    commands.spawn((
                        Sprite {
                            color: PLATFORM_COLOR,
                            custom_size: Some(Vec2::new(count * TILE_SIZE, TILE_SIZE)),
                            ..default()
                        },
                        Transform::from_xyz(world_x, world_y, 0.0),
                        Platform,
                    ));
                    continue;
                }
                'P' => {
                    let world_x = offset_x + col as f32 * TILE_SIZE;
                    let world_y = offset_y - row as f32 * TILE_SIZE;
                    player_spawn = Vec2::new(world_x, world_y);
                    col += 1;
                    continue;
                }
                _ => {
                    col += 1;
                }
            }
        }
    }

    commands.insert_resource(PlayerSpawn(player_spawn));
}