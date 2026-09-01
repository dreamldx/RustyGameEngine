# Migrate InputController to leafwing_input_manager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hand-rolled `InputController` (`ActionPressed`/`ActionReleased` messages, manual multi-key aggregation) with `leafwing_input_manager`, so `player_input`, `toggle_ui_visibility`, and `request_tuning_validation` read `ActionState<A>` components instead of subscribing to messages.

**Architecture:** Two `Actionlike` enums replace the single `InputAction` enum: `PlayerAction` (`Move` as a signed axis, `Jump`) attached to the player entity via `InputMap<PlayerAction>`, and `DebugAction` (`ToggleDebugUi`, `ValidateTuning`) attached to a new dedicated `DebugInputMarker` entity. `InputManagerPlugin::<A>` handles polling, multi-key merging, and press/release edge detection internally — no more manual `Local<HashMap<...>>` diffing or `.after(...)` ordering, since the crate's own `update_action_state` system already runs in `PreUpdate`, before this project's `Update` systems.

**Tech Stack:** Bevy `0.19`, `leafwing_input_manager = "0.21"` (already added to `Cargo.toml` and confirmed to build during design research — no new dependency step needed).

**Spec:** `docs/superpowers/specs/2026-09-01-leafwing-input-migration-design.md`

## Global Constraints

- `leafwing_input_manager = "0.21"` targets Bevy `0.19` — already present in `Cargo.toml`.
- `PlayerAction::Move` is a single signed axis (`#[actionlike(Axis)]`), not two booleans — bound to both `VirtualAxis::ad()` and `VirtualAxis::horizontal_arrow_keys()` so either key set drives it.
- `DebugAction` (and its `ActionState`) must live on a component-based entity, not a `Resource` — resource-only `ActionState` was removed in `0.21.0` (confirmed in the vendored source: `update_action_state<A>` only queries `(&mut ActionState<A>, &InputMap<A>)` as components).
- Key bindings, unchanged from before: `KeyA`/`ArrowLeft` → `Move` negative; `KeyD`/`ArrowRight` → `Move` positive; `Space` → `Jump`; `F1` → `ToggleDebugUi`; `F5` → `ValidateTuning`.
- Existing gameplay behavior (movement speed/direction, jump-only-when-grounded, F1 toggle, F5 validation) must be unchanged from the player's perspective.
- Because `input.rs`'s public API (`ActionPressed`, `ActionReleased`, `InputAction`, `InputSet`, `publish_input_actions`) is used by `systems.rs`, `debug_ui.rs`, `scripting.rs`, and `mod.rs`, this migration cannot be split into independently-compiling increments the way the original `InputController` build was — replacing `input.rs`'s API breaks all four other files simultaneously until they're all updated. This plan has one large task covering all five files, then a code-free integration-verification task, rather than the finer per-consumer task split used previously.

---

## Task 1: Migrate all input handling to leafwing_input_manager

**Files:**
- Rewrite: `src/engine/input.rs`
- Modify: `src/engine/player.rs`
- Modify: `src/engine/systems.rs`
- Modify: `src/engine/debug_ui.rs`
- Modify: `src/engine/scripting.rs`
- Modify: `src/engine/mod.rs`

**Interfaces:**
- Produces: `input::PlayerAction` (enum: `Move`, `Jump`), `input::DebugAction` (enum: `ToggleDebugUi`, `ValidateTuning`), `input::DebugInputMarker` (component), `input::player_input_map() -> InputMap<PlayerAction>`, `input::spawn_debug_input_map` (Startup system).
- Removes entirely: `input::InputAction`, `input::ActionPressed`, `input::ActionReleased`, `input::InputSet`, `input::publish_input_actions`.

- [ ] **Step 1: Rewrite `input.rs`**

Replace the entire contents of `src/engine/input.rs` with:

```rust
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect)]
pub enum PlayerAction {
    #[actionlike(Axis)]
    Move,
    Jump,
}

#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect)]
pub enum DebugAction {
    ToggleDebugUi,
    ValidateTuning,
}

/// The player's input bindings: `A`/`D` and the arrow keys both drive the
/// same `Move` axis (either can be held/released independently — the crate
/// merges them), `Space` triggers `Jump`.
pub fn player_input_map() -> InputMap<PlayerAction> {
    InputMap::default()
        .with_axis(PlayerAction::Move, VirtualAxis::ad())
        .with_axis(PlayerAction::Move, VirtualAxis::horizontal_arrow_keys())
        .with(PlayerAction::Jump, KeyCode::Space)
}

/// Marks the entity carrying `InputMap<DebugAction>`/`ActionState<DebugAction>`,
/// so consumers can query it specifically — `DebugAction` isn't tied to the
/// player entity, and resource-only `ActionState` isn't supported by this
/// version of the crate.
#[derive(Component)]
pub struct DebugInputMarker;

pub fn spawn_debug_input_map(mut commands: Commands) {
    commands.spawn((
        DebugInputMarker,
        InputMap::default()
            .with(DebugAction::ToggleDebugUi, KeyCode::F1)
            .with(DebugAction::ValidateTuning, KeyCode::F5),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::InputPlugin;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputPlugin,
            InputManagerPlugin::<PlayerAction>::default(),
        ));
        app.world_mut().spawn(player_input_map());
        app.update();
        app
    }

    fn action_state(app: &mut App) -> ActionState<PlayerAction> {
        let world = app.world_mut();
        let mut query = world.query::<&ActionState<PlayerAction>>();
        query.single(world).unwrap().clone()
    }

    #[test]
    fn move_axis_merges_ad_and_arrow_key_bindings() {
        let mut app = test_app();

        KeyCode::KeyD.press(app.world_mut());
        app.update();
        assert!(action_state(&mut app).clamped_value(&PlayerAction::Move) > 0.0);

        KeyCode::ArrowRight.press(app.world_mut());
        app.update();
        assert!(action_state(&mut app).clamped_value(&PlayerAction::Move) > 0.0);

        KeyCode::KeyD.release(app.world_mut());
        app.update();
        assert!(
            action_state(&mut app).clamped_value(&PlayerAction::Move) > 0.0,
            "releasing KeyD should not stop movement while ArrowRight is still held"
        );

        KeyCode::ArrowRight.release(app.world_mut());
        app.update();
        assert_eq!(
            action_state(&mut app).clamped_value(&PlayerAction::Move),
            0.0
        );
    }

    #[test]
    fn jump_action_detects_just_pressed() {
        let mut app = test_app();

        KeyCode::Space.press(app.world_mut());
        app.update();

        assert!(action_state(&mut app).just_pressed(&PlayerAction::Jump));
    }
}
```

These two tests verify *our binding configuration* (both key sets correctly drive `Move`, `Jump` is detected) — they are not re-testing the crate's internal merge/edge-detection logic, which is the crate's own responsibility and already covered by its test suite.

- [ ] **Step 2: Run input.rs's tests to verify they pass in isolation**

Run: `cargo test --lib engine::input`
Expected: FAIL to compile — the rest of the crate (`systems.rs`, `debug_ui.rs`, `scripting.rs`, `mod.rs`) still references the now-deleted `ActionPressed`/`ActionReleased`/`InputAction`/`InputSet`/`publish_input_actions`. This is expected at this point; continue to the next steps before the crate will compile at all. (There is no way to make this compile as an isolated increment — see Global Constraints.)

- [ ] **Step 3: Update `player.rs` to attach the player's input map**

In `src/engine/player.rs`, add the import and insert `player_input_map()` into the player's spawn bundle:

```rust
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
```

`ActionState<PlayerAction>` is not inserted manually — `InputMap<PlayerAction>` requires it as a dependent component, and it's added automatically.

- [ ] **Step 4: Rewrite `player_input` and its tests in `systems.rs`**

Replace the top of `src/engine/systems.rs` (imports through the end of `player_input`):

```rust
use crate::engine::components::*;
use crate::engine::input::PlayerAction;
use crate::engine::scripting::ScriptedTuning;
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

pub fn player_input(
    tuning: Res<ScriptedTuning>,
    mut query: Query<(&ActionState<PlayerAction>, &mut Velocity, &Grounded), With<Player>>,
) {
    let Ok((action_state, mut velocity, grounded)) = query.single_mut() else {
        return;
    };

    let dir = action_state.clamped_value(&PlayerAction::Move);
    velocity.0.x = dir * tuning.move_speed;

    if action_state.just_pressed(&PlayerAction::Jump) && grounded.0 {
        velocity.0.y = tuning.jump_force;
    }
}
```

This deletes the `MoveDirState` struct and its `Local` entirely — the crate's `ActionState` already holds the equivalent held/edge state.

Leave `apply_gravity`, `apply_velocity`, `ground_detection`, and `camera_follow` unchanged.

At the top of the `#[cfg(test)] mod tests` block at the bottom of the file, replace this line:

```rust
    use crate::engine::input::{ActionPressed, ActionReleased, InputAction};
```

with:

```rust
    use crate::engine::input::player_input_map;
    use bevy::input::InputPlugin;
    use leafwing_input_manager::prelude::*;
```

(leave the existing `use super::*;` and `use crate::engine::scripting::ScriptedTuning;` lines as they are).

Then replace the `player_input_uses_scripted_move_speed` and `player_input_uses_scripted_jump_force` tests themselves:

```rust
    fn player_test_app(tuning: ScriptedTuning) -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputPlugin,
            InputManagerPlugin::<PlayerAction>::default(),
        ));
        app.insert_resource(tuning);
        app.add_systems(Update, player_input);
        app
    }

    #[test]
    fn player_input_uses_scripted_move_speed() {
        let mut app = player_test_app(ScriptedTuning {
            move_speed: 250.0,
            jump_force: 500.0,
            gravity: 980.0,
        });
        let entity = app
            .world_mut()
            .spawn((
                Player,
                Velocity::default(),
                Grounded(false),
                player_input_map(),
            ))
            .id();

        KeyCode::KeyD.press(app.world_mut());
        app.update();

        let velocity = app.world().get::<Velocity>(entity).unwrap();
        assert_eq!(velocity.0.x, 250.0);
    }

    #[test]
    fn player_input_uses_scripted_jump_force() {
        let mut app = player_test_app(ScriptedTuning {
            move_speed: 200.0,
            jump_force: 650.0,
            gravity: 980.0,
        });
        let entity = app
            .world_mut()
            .spawn((
                Player,
                Velocity::default(),
                Grounded(true),
                player_input_map(),
            ))
            .id();

        KeyCode::Space.press(app.world_mut());
        app.update();

        let velocity = app.world().get::<Velocity>(entity).unwrap();
        assert_eq!(velocity.0.y, 650.0);
    }
```

Leave the existing `use super::*;` line at the top of the `tests` module and the unchanged `apply_gravity_uses_scripted_gravity` test as they are — just add the new imports and replace the two tests shown above.

- [ ] **Step 5: Rewrite `toggle_ui_visibility` and its test in `debug_ui.rs`**

In `src/engine/debug_ui.rs`, change the import line:

```rust
use crate::engine::input::{DebugAction, DebugInputMarker};
```

(replacing `use crate::engine::input::{ActionPressed, InputAction};`), and add `use leafwing_input_manager::prelude::*;` alongside the other `use` lines near the top of the file.

Replace `toggle_ui_visibility`:

```rust
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
```

Replace the `toggle_ui_visibility_flips_on_toggle_action` test in the `#[cfg(test)] mod tests` block:

```rust
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
```

- [ ] **Step 6: Rewrite `request_tuning_validation` and its test in `scripting.rs`**

In `src/engine/scripting.rs`, change the import line:

```rust
use crate::engine::input::{DebugAction, DebugInputMarker};
```

(replacing `use crate::engine::input::{ActionPressed, InputAction};`), and add `use leafwing_input_manager::prelude::*;` alongside the other `use` lines near the top of the file.

Replace `request_tuning_validation`:

```rust
pub fn request_tuning_validation(
    debug_input: Query<&ActionState<DebugAction>, With<DebugInputMarker>>,
    query: Query<(Entity, &ScriptComponent), With<PlayerTuningScript>>,
    mut callbacks: MessageWriter<ScriptCallbackEvent>,
) {
    let Ok(debug_action_state) = debug_input.single() else {
        return;
    };
    if !debug_action_state.just_pressed(&DebugAction::ValidateTuning) {
        return;
    }
    let Ok((entity, script)) = query.single() else {
        warn!("ValidateTuning pressed but no player tuning script is loaded yet");
        return;
    };
    let Some(handle) = script.0.first() else {
        return;
    };
    callbacks.write(
        ScriptCallbackEvent::new_for_script_entity(OnGetTuning, vec![], handle.clone(), entity)
            .with_response(),
    );
}
```

Replace the `request_tuning_validation_sends_callback_on_validate_action` test inside `#[cfg(test)] mod tuning_validation_tests`:

```rust
    #[test]
    fn request_tuning_validation_sends_callback_on_validate_action() {
        use crate::engine::input::{DebugAction, DebugInputMarker};
        use bevy::input::InputPlugin;
        use bevy_mod_scripting::prelude::ScriptAsset;
        use bevy_mod_scripting_core::event::CallbackLabel;
        use leafwing_input_manager::prelude::*;

        #[derive(Resource, Default)]
        struct CapturedCallbacks(Vec<CallbackLabel>);

        fn capture_callbacks(
            mut reader: MessageReader<ScriptCallbackEvent>,
            mut captured: ResMut<CapturedCallbacks>,
        ) {
            for event in reader.read() {
                captured.0.push(event.label.clone());
            }
        }

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputPlugin,
            InputManagerPlugin::<DebugAction>::default(),
        ));
        app.add_message::<ScriptCallbackEvent>();
        app.insert_resource(CapturedCallbacks::default());

        app.world_mut().spawn((
            DebugInputMarker,
            InputMap::default().with(DebugAction::ValidateTuning, KeyCode::F5),
        ));

        let mut script_assets = Assets::<ScriptAsset>::default();
        let handle = script_assets.add(ScriptAsset::new(String::new()));
        app.insert_resource(script_assets);
        app.world_mut()
            .spawn((ScriptComponent(vec![handle]), PlayerTuningScript));

        app.add_systems(
            Update,
            (request_tuning_validation, capture_callbacks).chain(),
        );

        KeyCode::F5.press(app.world_mut());
        app.update();

        let captured = app.world().resource::<CapturedCallbacks>();
        assert_eq!(captured.0, vec![OnGetTuning.into()]);
    }
```

Leave `describe_tuning_mismatch_returns_none_when_equal` and `describe_tuning_mismatch_reports_differing_fields` untouched — they don't involve input at all.

- [ ] **Step 7: Rewrite `mod.rs`'s plugin registration**

Replace `src/engine/mod.rs` in full:

```rust
pub mod components;
pub mod debug_ui;
pub mod input;
pub mod level;
pub mod player;
pub mod scripting;
pub mod systems;

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use bevy_mod_scripting::prelude::*;
use debug_ui::{ReloadLevelRequested, UiVisible};
use input::{DebugAction, PlayerAction};
use leafwing_input_manager::prelude::InputManagerPlugin;
use scripting::ScriptedTuning;

pub struct EnginePlugin;

impl Plugin for EnginePlugin {
    fn build(&self, app: &mut App) {
        scripting::register_functions(app.world_mut());

        app.add_plugins(BMSPlugin)
            .add_plugins(EguiPlugin::default())
            .add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_plugins(InputManagerPlugin::<PlayerAction>::default())
            .add_plugins(InputManagerPlugin::<DebugAction>::default())
            .init_resource::<ScriptedTuning>()
            .init_resource::<UiVisible>()
            .init_resource::<ReloadLevelRequested>()
            .init_resource::<debug_ui::AssetTree>()
            .add_systems(
                Startup,
                (
                    level::spawn_level,
                    player::spawn_player,
                    player::spawn_camera,
                    scripting::load_scripts,
                    debug_ui::build_asset_tree,
                    input::spawn_debug_input_map,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    systems::player_input,
                    systems::apply_gravity,
                    systems::ground_detection,
                    systems::apply_velocity,
                    systems::camera_follow,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    scripting::request_tuning_validation,
                    scripting::handle_tuning_validation_response,
                    event_handler::<scripting::OnGetTuning, LuaScriptingPlugin>,
                ),
            )
            .add_systems(
                Update,
                (debug_ui::toggle_ui_visibility, debug_ui::apply_level_reload),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (debug_ui::draw_debug_ui, debug_ui::draw_panels_ui),
            );
    }
}
```

Note there's no `.after(...)` anywhere — `InputManagerPlugin`'s `update_action_state` system runs in `PreUpdate`, which always completes before `Update` starts, so every consumer here already sees fresh `ActionState` with no manual ordering needed.

- [ ] **Step 8: Build**

Run: `cargo build`
Expected: succeeds. If it doesn't, the error will point at a leftover reference to the old `ActionPressed`/`ActionReleased`/`InputAction`/`InputSet`/`publish_input_actions` API — search the file named in the error for anything still importing from `crate::engine::input` with those old names and update it per the steps above.

- [ ] **Step 9: Run the full test suite**

Run: `cargo test`
Expected: all pass — 2 tests from `input.rs` (new), 3 from `systems.rs` (2 rewritten + `apply_gravity_uses_scripted_gravity` unchanged), 1 from `debug_ui.rs` (rewritten), 3 from `scripting.rs` (2 unchanged `describe_tuning_mismatch_*` + 1 rewritten). 9 tests total, same count as before the migration.

- [ ] **Step 10: Manual QA**

Run: `cargo run`. Confirm:
1. A/D and arrow keys move the player left/right at the same speed as before, including holding both key sets together and releasing only one.
2. Space jumps only while grounded.
3. F1 still toggles the Debug window, menu bar, asset sidebar, and status bar together.
4. F5 still triggers the tuning validation log (`info!`/`warn!` match/mismatch messages).
5. The "Reload Level" and "Exit Game" menu buttons still work (unaffected by this migration, but `mod.rs` changed, so worth a sanity check).

- [ ] **Step 11: Commit**

```bash
git add Cargo.toml Cargo.lock src/engine/input.rs src/engine/player.rs src/engine/systems.rs src/engine/debug_ui.rs src/engine/scripting.rs src/engine/mod.rs
git commit -m "Migrate InputController to leafwing_input_manager"
```

(Skip this step if git isn't set up in this environment yet — leave the changes uncommitted.)

---

## Task 2: Integration verification

No code changes — confirms the migration is complete and consistent, since Task 1's steps were verified individually as they were written but not all together as a final pass.

**Files:** none.

- [ ] **Step 1: Confirm no references to the deleted API remain**

Run: `grep -rn "ActionPressed\|ActionReleased\|InputAction\b\|InputSet\|publish_input_actions" src/`
Expected: no matches anywhere in `src/`.

- [ ] **Step 2: Confirm `ButtonInput` is no longer read directly anywhere in `src/engine`**

Run: `grep -rln "ButtonInput" src/engine/`
Expected: no matches — all keyboard reads now go through `leafwing_input_manager`'s `InputMap`/`ActionState`, not raw `ButtonInput<KeyCode>`.

- [ ] **Step 3: Full test suite one more time**

Run: `cargo test`
Expected: all 9 tests pass.

- [ ] **Step 4: Commit** (only if Task 1's commit step was skipped and everything above passes)

```bash
git status
```

If there are uncommitted changes because no git identity was configured earlier, this is the point to either configure git identity and commit everything from this migration, or leave it for the user — do not force a commit without one of those two being true.
