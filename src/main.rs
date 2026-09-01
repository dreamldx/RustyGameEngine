mod engine;

use bevy::prelude::*;
use engine::EnginePlugin;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, EnginePlugin))
        .run();
}