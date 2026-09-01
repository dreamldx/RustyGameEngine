use crate::engine::components::{Platform, Player};
use crate::engine::input::{DebugAction, DebugInputMarker};
use crate::engine::scripting::ScriptedTuning;
use crate::engine::{level, player};
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_mod_scripting::prelude::ScriptComponent;
use egui::{LayerId, Ui, UiBuilder};
use leafwing_input_manager::prelude::*;
use std::fs;
use std::path::Path;

#[derive(Resource, Default)]
pub struct UiVisible(pub bool);

#[derive(Resource, Default)]
pub struct ReloadLevelRequested(pub bool);

/// A file or directory under `assets/`, scanned once at startup.
pub enum FileNode {
    File(String),
    Dir(String, Vec<FileNode>),
}

#[derive(Resource, Default)]
pub struct AssetTree(pub Vec<FileNode>);

fn scan_dir(dir: &Path) -> Vec<FileNode> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = read_dir.flatten().map(|entry| entry.path()).collect();
    paths.sort();

    paths
        .into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_string();
            Some(if path.is_dir() {
                FileNode::Dir(name, scan_dir(&path))
            } else {
                FileNode::File(name)
            })
        })
        .collect()
}

/// Scans `assets/` once at startup into `AssetTree`, for the read-only
/// sidebar. Does not refresh while the app is running.
pub fn build_asset_tree(mut commands: Commands) {
    commands.insert_resource(AssetTree(scan_dir(Path::new("assets"))));
}

fn draw_file_nodes(ui: &mut egui::Ui, nodes: &[FileNode]) {
    for node in nodes {
        match node {
            FileNode::File(name) => {
                ui.label(name);
            }
            FileNode::Dir(name, children) => {
                egui::CollapsingHeader::new(name)
                    .default_open(false)
                    .show(ui, |ui| {
                        draw_file_nodes(ui, children);
                    });
            }
        }
    }
}

pub fn toggle_ui_visibility(
    query: Query<&ActionState<DebugAction>, With<DebugInputMarker>>,
    mut visible: ResMut<UiVisible>,
) {
    let Ok(action_state) = query.single() else {
        return;
    };
    if action_state.just_pressed(&DebugAction::ToggleDebugUi) {
        visible.0 = !visible.0;
    }
}

pub fn draw_debug_ui(
    visible: Res<UiVisible>,
    tuning: Res<ScriptedTuning>,
    mut contexts: EguiContexts,
) -> Result {
    if !visible.0 {
        return Ok(());
    }

    egui::Window::new("Debug").show(contexts.ctx_mut()?, |ui| {
        ui.label(format!("move_speed: {}", tuning.move_speed));
        ui.label(format!("jump_force: {}", tuning.jump_force));
        ui.label(format!("gravity: {}", tuning.gravity));
    });

    Ok(())
}

/// Draws the top menu bar, bottom status bar, and right asset sidebar in a
/// single shared root `Ui`. Panel order matters for layout (each panel
/// insets the remaining space for the ones after it) — top and bottom are
/// added first so they span the full window width; the asset sidebar is
/// added last so it only fills the space between them, not the corners.
pub fn draw_panels_ui(
    visible: Res<UiVisible>,
    mut contexts: EguiContexts,
    mut reload_requested: ResMut<ReloadLevelRequested>,
    mut exit: MessageWriter<AppExit>,
    asset_tree: Res<AssetTree>,
    diagnostics: Res<DiagnosticsStore>,
    scripts: Query<&ScriptComponent>,
) -> Result {
    if !visible.0 {
        return Ok(());
    }

    let ctx = contexts.ctx_mut()?;
    let mut root_ui = Ui::new(
        ctx.clone(),
        "debug_panels_root".into(),
        UiBuilder::new()
            .layer_id(LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    egui::Panel::top("menu_bar").show(&mut root_ui, |ui| {
        ui.horizontal(|ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Reload Level").clicked() {
                    reload_requested.0 = true;
                    ui.close();
                }
                if ui.button("Exit Game").clicked() {
                    exit.write(AppExit::Success);
                    ui.close();
                }
            });
        });
    });

    egui::Panel::bottom("status_bar").show(&mut root_ui, |ui| {
        let fps = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|d| d.smoothed())
            .unwrap_or(0.0);
        let script_count: usize = scripts.iter().map(|s| s.0.len()).sum();
        ui.horizontal(|ui| {
            ui.label(format!("FPS: {fps:.0}"));
            ui.separator();
            ui.label(format!("Scripts loaded: {script_count}"));
        });
    });

    egui::Panel::right("asset_panel")
        .default_size(300.0)
        .min_size(300.0)
        .show(&mut root_ui, |ui| {
        ui.heading("Assets");
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
            draw_file_nodes(ui, &asset_tree.0);
        });
    });

    Ok(())
}

/// Despawns all `Platform`/`Player` entities and re-runs `spawn_level` and
/// `spawn_player`, when `ReloadLevelRequested` was set by the "Reload Level"
/// button. Runs with direct `&mut World` access so the despawn and the
/// respawn happen in the same frame, with no deferred-command ordering to
/// coordinate.
pub fn apply_level_reload(world: &mut World) {
    let should_reload = {
        let mut requested = world.resource_mut::<ReloadLevelRequested>();
        let should_reload = requested.0;
        requested.0 = false;
        should_reload
    };
    if !should_reload {
        return;
    }

    let mut platform_query = world.query_filtered::<Entity, With<Platform>>();
    let platforms: Vec<Entity> = platform_query.iter(world).collect();
    let mut player_query = world.query_filtered::<Entity, With<Player>>();
    let players: Vec<Entity> = player_query.iter(world).collect();
    for entity in platforms.into_iter().chain(players) {
        world.despawn(entity);
    }

    if let Err(e) = world.run_system_once(level::spawn_level) {
        error!("Failed to respawn level: {e}");
        return;
    }
    if let Err(e) = world.run_system_once(player::spawn_player) {
        error!("Failed to respawn player: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::InputPlugin;

    #[test]
    fn toggle_ui_visibility_flips_on_toggle_action() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputPlugin,
            InputManagerPlugin::<DebugAction>::default(),
        ));
        app.insert_resource(UiVisible(false));
        app.world_mut().spawn((
            DebugInputMarker,
            InputMap::default().with(DebugAction::ToggleDebugUi, KeyCode::F1),
        ));
        app.add_systems(Update, toggle_ui_visibility);

        KeyCode::F1.press(app.world_mut());
        app.update();

        assert!(app.world().resource::<UiVisible>().0);
    }
}
