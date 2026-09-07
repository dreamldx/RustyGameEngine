use crate::engine::components::Player;
use crate::engine::input::{self, DebugAction, DebugInputMarker};
use crate::engine::level;
use crate::engine::scripting::ScriptedTuning;
use crate::engine::systems;
use bevy::diagnostic::{
    DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
    SystemInformationDiagnosticsPlugin, SystemInfo,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use bevy_mod_scripting::prelude::ScriptComponent;
use egui::{LayerId, Ui, UiBuilder};
use leafwing_input_manager::prelude::*;
use std::fs;
use std::path::Path;

#[derive(Resource, Default)]
pub struct UiVisible(pub bool);

/// Visibility of the "Debug" tuning window specifically, toggled from the
/// menu bar's Window menu — independent of `UiVisible`, which gates the
/// whole dev overlay (menu bar included). Off by default.
#[derive(Resource, Default)]
pub struct DebugWindowVisible(pub bool);

/// Visibility of the level-bounds gizmo (see `draw_bounds_gizmo`), toggled
/// from the menu bar's Window menu. Off by default.
#[derive(Resource, Default)]
pub struct BoundsGizmoVisible(pub bool);

#[derive(Resource, Default)]
pub struct ReloadLevelRequested(pub bool);

/// Set by double-clicking a level file in the asset panel; consumed by
/// `apply_level_reload`, which loads that level and updates `CurrentLevel`.
#[derive(Resource, Default)]
pub struct LoadLevelRequested(pub Option<String>);

/// The name of the level last loaded (via double-click or "Reload Level"),
/// so "Reload Level" knows what to reload once more than one level exists.
#[derive(Resource)]
pub struct CurrentLevel(pub String);

impl Default for CurrentLevel {
    fn default() -> Self {
        Self("main".to_string())
    }
}

pub struct DebugUiPlugin;

impl Plugin for DebugUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiVisible>()
            .init_resource::<DebugWindowVisible>()
            .init_resource::<BoundsGizmoVisible>()
            .init_resource::<LoadLevelRequested>()
            .init_resource::<CurrentLevel>()
            .add_systems(
                Startup,
                (build_asset_tree, input::spawn_debug_input_map).chain(),
        )
        .add_systems(Update, (toggle_ui_visibility, apply_level_reload, draw_bounds_gizmo))
        .add_systems(EguiPrimaryContextPass, (draw_debug_ui, draw_panels_ui));
    }
}

/// A file or directory under `assets/`, scanned once at startup.
pub enum FileNode {
    File(String),
    Dir(String, Vec<FileNode>),
}

#[derive(Component, Default)]
pub struct AssetTree(pub Vec<FileNode>);

fn scan_dir(dir: &Path) -> Vec<FileNode> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = read_dir.flatten().map(|entry| entry.path()).collect();
    // Directories before files, alphabetically within each group.
    paths.sort_by(|a, b| {
        (a.is_file(), a.file_name()).cmp(&(b.is_file(), b.file_name()))
    });

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

/// Scans `assets/` once at startup into an `AssetTree` entity, for the
/// read-only sidebar. Does not refresh while the app is running.
pub fn build_asset_tree(mut commands: Commands) {
    commands.spawn(AssetTree(scan_dir(Path::new("assets"))));
}

/// Draws the asset tree. `in_levels_dir` marks recursion inside the
/// top-level `levels/` folder, where files are double-clickable to load as
/// the current level; returns the double-clicked file's name (minus
/// extension) if one was double-clicked this frame.
fn draw_file_nodes(ui: &mut egui::Ui, nodes: &[FileNode], in_levels_dir: bool) -> Option<String> {
    let mut loaded = None;
    for node in nodes {
        match node {
            FileNode::File(name) => {
                if in_levels_dir {
                    let response = ui.add(egui::Label::new(name).sense(egui::Sense::click()));
                    if response.double_clicked() {
                        let level_name = name.rsplit_once('.').map_or(name.as_str(), |(stem, _)| stem);
                        loaded = Some(level_name.to_string());
                    }
                } else {
                    ui.label(name);
                }
            }
            FileNode::Dir(name, children) => {
                // Not `in_levels_dir || name == "levels"` — `load_level_scripts`
                // only scans the immediate contents of `assets/levels` via
                // `fs::read_dir` (non-recursive), so a file nested any deeper
                // isn't a loadable level and shouldn't be clickable as one.
                let child_in_levels = name == "levels";
                egui::CollapsingHeader::new(name)
                    .default_open(false)
                    .show(ui, |ui| {
                        if let Some(name) = draw_file_nodes(ui, children, child_in_levels) {
                            loaded = Some(name);
                        }
                    });
            }
        }
    }
    loaded
}

/// Draws the current level's horizontal bounds (two vertical lines at
/// `LevelBounds::min_x`/`max_x`) and the fall-death respawn line (a
/// horizontal line at `systems::FALL_DEATH_Y`, spanning the level width),
/// when the "Bounds Gizmo" checkbox is on.
pub fn draw_bounds_gizmo(
    visible: Res<BoundsGizmoVisible>,
    bounds: Res<level::LevelBounds>,
    mut gizmos: Gizmos,
) {
    if !visible.0 {
        return;
    }

    let bounds_color = Color::srgb(0.2, 0.8, 1.0);
    let respawn_color = Color::srgb(1.0, 0.2, 0.2);
    // Tall enough to stay visible however far the player wanders vertically,
    // without needing to know the level's actual vertical extent.
    let top = 2000.0;
    let bottom = systems::FALL_DEATH_Y - 200.0;

    gizmos.line_2d(Vec2::new(bounds.min_x, bottom), Vec2::new(bounds.min_x, top), bounds_color);
    gizmos.line_2d(Vec2::new(bounds.max_x, bottom), Vec2::new(bounds.max_x, top), bounds_color);
    gizmos.line_2d(
        Vec2::new(bounds.min_x, systems::FALL_DEATH_Y),
        Vec2::new(bounds.max_x, systems::FALL_DEATH_Y),
        respawn_color,
    );
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
    debug_window_visible: Res<DebugWindowVisible>,
    tuning_query: Query<&ScriptedTuning, With<Player>>,
    mut contexts: EguiContexts,
) -> Result {
    if !visible.0 || !debug_window_visible.0 {
        return Ok(());
    }
    let Ok(tuning) = tuning_query.single() else {
        return Ok(());
    };

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
    mut debug_window_visible: ResMut<DebugWindowVisible>,
    mut bounds_gizmo_visible: ResMut<BoundsGizmoVisible>,
    mut contexts: EguiContexts,
    mut reload_requested: ResMut<ReloadLevelRequested>,
    mut load_level_requested: ResMut<LoadLevelRequested>,
    current_level: Res<CurrentLevel>,
    mut exit: MessageWriter<AppExit>,
    asset_tree_query: Query<&AssetTree>,
    diagnostics: Res<DiagnosticsStore>,
    system_info: Res<SystemInfo>,
    scripts: Query<&ScriptComponent>,
) -> Result {
    if !visible.0 {
        return Ok(());
    }
    let empty_asset_tree = Vec::new();
    let asset_tree_nodes: &[FileNode] = asset_tree_query
        .single()
        .map(|tree| tree.0.as_slice())
        .unwrap_or(&empty_asset_tree);

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
            ui.menu_button("Window", |ui| {
                ui.checkbox(&mut debug_window_visible.0, "Debug Window");
                ui.checkbox(&mut bounds_gizmo_visible.0, "Bounds Gizmo");
            });
        });
    });

    egui::Panel::bottom("status_bar").show(&mut root_ui, |ui| {
        let fps = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|d| d.smoothed())
            .unwrap_or(0.0);
        let frame_time = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
            .and_then(|d| d.smoothed())
            .unwrap_or(0.0);
        let entity_count = diagnostics
            .get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT)
            .and_then(|d| d.value())
            .unwrap_or(0.0);
        let cpu_usage = diagnostics
            .get(&SystemInformationDiagnosticsPlugin::SYSTEM_CPU_USAGE)
            .and_then(|d| d.smoothed())
            .unwrap_or(0.0);
        let mem_usage = diagnostics
            .get(&SystemInformationDiagnosticsPlugin::PROCESS_MEM_USAGE)
            .and_then(|d| d.smoothed())
            .unwrap_or(0.0);
        let script_count: usize = scripts.iter().map(|s| s.0.len()).sum();

        ui.horizontal(|ui| {
            ui.label(format!("Level: {}", current_level.0));
            ui.separator();
            ui.label(format!("FPS: {fps:.0}"));
            ui.separator();
            ui.label(format!("Frame: {frame_time:.1}ms"));
            ui.separator();
            ui.label(format!("Entities: {entity_count:.0}"));
            ui.separator();
            ui.label(format!("Scripts: {script_count}"));
            ui.separator();
            ui.label(format!("CPU: {cpu_usage:.0}%"));
            ui.separator();
            ui.label(format!("Mem: {mem_usage:.1} GiB"));
            ui.separator();
            ui.label(format!(
                "{} | {} cores | {} | {}",
                system_info.os,
                system_info.core_count,
                system_info.cpu,
                system_info.memory,
            ));
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
            if let Some(name) = draw_file_nodes(ui, asset_tree_nodes, false) {
                load_level_requested.0 = Some(name);
            }
        });
    });

    Ok(())
}

/// Despawns the current level's `Platform`/`Player` entities and re-requests
/// a level's `on_level_load()`, when `LoadLevelRequested` names a specific level
/// (double-clicked in the asset panel) or `ReloadLevelRequested` was set by
/// the "Reload Level" button (reloads `CurrentLevel`). The actual respawn
/// happens asynchronously once the level script's `on_level_load()` callback responds
/// (see `level.rs`).
pub fn apply_level_reload(world: &mut World) {
    let requested_level = {
        let mut requested = world.resource_mut::<LoadLevelRequested>();
        requested.0.take()
    };

    let name = if let Some(name) = requested_level {
        Some(name)
    } else {
        let should_reload = {
            let mut reload = world.resource_mut::<ReloadLevelRequested>();
            let should_reload = reload.0;
            reload.0 = false;
            should_reload
        };
        should_reload.then(|| world.resource::<CurrentLevel>().0.clone())
    };

    let Some(name) = name else {
        return;
    };

    world.resource_mut::<CurrentLevel>().0 = name.clone();
    level::reload_level(world, &name);
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

    #[test]
    fn apply_level_reload_loads_requested_level_and_updates_current() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ReloadLevelRequested>();
        app.insert_resource(LoadLevelRequested(Some("other".to_string())));
        app.insert_resource(CurrentLevel("main".to_string()));
        app.init_resource::<level::LevelLoadRequested>();
        app.add_systems(Update, apply_level_reload);

        app.update();

        assert_eq!(app.world().resource::<CurrentLevel>().0, "other");
        assert!(app.world().resource::<LoadLevelRequested>().0.is_none());
        assert_eq!(
            app.world().resource::<level::LevelLoadRequested>().0,
            Some("other".to_string())
        );
    }

    #[test]
    fn apply_level_reload_falls_back_to_current_level_when_no_specific_request() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(ReloadLevelRequested(true));
        app.init_resource::<LoadLevelRequested>();
        app.insert_resource(CurrentLevel("main".to_string()));
        app.init_resource::<level::LevelLoadRequested>();
        app.add_systems(Update, apply_level_reload);

        app.update();

        assert_eq!(app.world().resource::<CurrentLevel>().0, "main");
        assert_eq!(
            app.world().resource::<level::LevelLoadRequested>().0,
            Some("main".to_string())
        );
    }
}
