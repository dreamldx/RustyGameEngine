mod engine;

use bevy::prelude::*;
use bevy::window::WindowResolution;
use engine::EnginePlugin;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    resolution: WindowResolution::new(1920, 1080),
                    ..default()
                }),
                ..default()
            }),
            EnginePlugin,
        ))
        .run();
}