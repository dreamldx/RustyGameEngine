# Input Controller (Pub/Sub Action Messages) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Centralize all keyboard polling into one `InputController` publisher (`src/engine/input.rs`) that emits semantic `ActionPressed`/`ActionReleased` messages, and refactor the three existing direct-`ButtonInput` consumers (`player_input`, `toggle_ui_visibility`, `request_tuning_validation`) to subscribe to those messages instead.

**Architecture:** A new `publish_input_actions` system reads `ButtonInput<KeyCode>` each frame, aggregates a fixed key→action binding table per `InputAction` (so two keys bound to the same action don't produce spurious release events), and writes `ActionPressed`/`ActionReleased` messages on state transitions. It's placed in a `InputSet` system set that every consumer is ordered `.after()`, so messages are available same-frame. Each consumer keeps only the state it actually needs (movement direction as two held-flags; jump/toggle/validate as one-shot reads).

**Tech Stack:** Bevy `0.19` (`Message`/`MessageReader`/`MessageWriter`, the renamed Event system), no new external dependencies.

**Spec:** `docs/superpowers/specs/2026-08-31-input-controller-design.md`

## Global Constraints

- Key bindings (exact, from the spec): `KeyA`/`ArrowLeft` → `MoveLeft`; `KeyD`/`ArrowRight` → `MoveRight`; `Space` → `Jump`; `F1` → `ToggleDebugUi`; `F5` → `ValidateTuning`.
- `publish_input_actions` must aggregate per-action (not forward per-key) — releasing one of two keys bound to the same action must not emit `ActionReleased` while the other bound key is still held.
- `publish_input_actions` must run before every consumer in the same frame (no one-frame input lag).
- No gamepad/mouse support, no remappable/config-file bindings — the binding table is a fixed Rust `const`.
- Existing gameplay behavior (movement speed/direction, jump-only-when-grounded, F1 toggle, F5 validation) must be unchanged from the player's perspective — this is a refactor of *how* input reaches these systems, not a behavior change.

---

## Task 1: `InputController` core — action messages and the publisher

**Files:**
- Create: `src/engine/input.rs`
- Modify: `src/engine/mod.rs`

**Interfaces:**
- Produces: `input::InputAction` enum (`Copy, Clone, PartialEq, Eq, Hash, Debug`; variants `MoveLeft, MoveRight, Jump, ToggleDebugUi, ValidateTuning`), `input::ActionPressed(pub InputAction)` (Message), `input::ActionReleased(pub InputAction)` (Message), `input::InputSet` (a `SystemSet` marker), `input::publish_input_actions` (system fn). These are consumed by Tasks 2–4.

- [ ] **Step 1: Write the failing test**

Create `src/engine/input.rs` with just enough to make the test module compile against real (not-yet-correct) types — write the whole file in one pass since the test and implementation are small and tightly coupled here; this is Step 1 and Step 3 combined for this task, which is acceptable per the task's own single coherent deliverable. Write this exact content:

```rust
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum InputAction {
    MoveLeft,
    MoveRight,
    Jump,
    ToggleDebugUi,
    ValidateTuning,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct ActionPressed(pub InputAction);

#[derive(Message, Clone, Copy, Debug)]
pub struct ActionReleased(pub InputAction);

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct InputSet;

const KEY_BINDINGS: &[(KeyCode, InputAction)] = &[
    (KeyCode::KeyA, InputAction::MoveLeft),
    (KeyCode::ArrowLeft, InputAction::MoveLeft),
    (KeyCode::KeyD, InputAction::MoveRight),
    (KeyCode::ArrowRight, InputAction::MoveRight),
    (KeyCode::Space, InputAction::Jump),
    (KeyCode::F1, InputAction::ToggleDebugUi),
    (KeyCode::F5, InputAction::ValidateTuning),
];

const ALL_ACTIONS: &[InputAction] = &[
    InputAction::MoveLeft,
    InputAction::MoveRight,
    InputAction::Jump,
    InputAction::ToggleDebugUi,
    InputAction::ValidateTuning,
];

/// Reads raw keyboard state and publishes `ActionPressed`/`ActionReleased`
/// on transitions of the *aggregated* per-action held-state (any bound key
/// held counts), not per physical key — see the design doc for why.
pub fn publish_input_actions(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut held: Local<HashMap<InputAction, bool>>,
    mut pressed: MessageWriter<ActionPressed>,
    mut released: MessageWriter<ActionReleased>,
) {
    for &action in ALL_ACTIONS {
        let is_held = KEY_BINDINGS
            .iter()
            .any(|(key, bound_action)| *bound_action == action && keyboard.pressed(*key));
        let was_held = held.get(&action).copied().unwrap_or(false);

        if is_held && !was_held {
            pressed.write(ActionPressed(action));
        } else if !is_held && was_held {
            released.write(ActionReleased(action));
        }
        held.insert(action, is_held);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Captured {
        pressed: Vec<InputAction>,
        released: Vec<InputAction>,
    }

    fn capture_messages(
        mut pressed: MessageReader<ActionPressed>,
        mut released: MessageReader<ActionReleased>,
        mut captured: ResMut<Captured>,
    ) {
        for ActionPressed(action) in pressed.read() {
            captured.pressed.push(*action);
        }
        for ActionReleased(action) in released.read() {
            captured.released.push(*action);
        }
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_message::<ActionPressed>();
        app.add_message::<ActionReleased>();
        app.insert_resource(Captured::default());
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.add_systems(Update, (publish_input_actions, capture_messages).chain());
        app
    }

    #[test]
    fn move_left_ignores_release_while_synonym_key_still_held() {
        let mut app = test_app();

        // Frame 1: press A -> MoveLeft pressed.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyA);
        app.update();

        // Frame 2: also press ArrowLeft while A is still held -> no new message.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowLeft);
        app.update();

        // Frame 3: release A, ArrowLeft still held -> must NOT release MoveLeft.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyA);
        app.update();

        // Frame 4: release ArrowLeft too -> MoveLeft finally releases.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::ArrowLeft);
        app.update();

        let captured = app.world().resource::<Captured>();
        assert_eq!(captured.pressed, vec![InputAction::MoveLeft]);
        assert_eq!(captured.released, vec![InputAction::MoveLeft]);
    }

    #[test]
    fn jump_pressed_and_released_with_single_key() {
        let mut app = test_app();

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::Space);
        app.update();

        let captured = app.world().resource::<Captured>();
        assert_eq!(captured.pressed, vec![InputAction::Jump]);
        assert_eq!(captured.released, vec![InputAction::Jump]);
    }
}
```

- [ ] **Step 2: Run the test to verify the dedup behavior actually needs the aggregation logic**

Run: `cargo test --lib engine::input`
Expected: PASS (the implementation above is already correct and complete — this step's real purpose is to confirm both tests pass as written; if `move_left_ignores_release_while_synonym_key_still_held` fails, the aggregation logic has a bug to fix before continuing).

- [ ] **Step 3: Wire the module in and register the messages**

In `src/engine/mod.rs`, add the module declaration near the others:

```rust
pub mod input;
```

In `EnginePlugin::build`, register the two message types and the publisher system (no ordering yet — that's added incrementally in Tasks 2–4 as each consumer migrates):

```rust
app.add_message::<input::ActionPressed>()
    .add_message::<input::ActionReleased>()
    .add_systems(Update, input::publish_input_actions.in_set(input::InputSet))
```

Chain this onto the existing `app.add_plugins(...)...` builder chain in whatever position keeps the file readable — order relative to the other `.add_systems`/`.init_resource` calls doesn't matter here.

- [ ] **Step 4: Build and run the full test suite**

Run: `cargo build`
Expected: succeeds.

Run: `cargo test`
Expected: all tests pass, including the two new ones and the five pre-existing ones (untouched so far).

- [ ] **Step 5: Commit**

```bash
git add src/engine/input.rs src/engine/mod.rs
git commit -m "Add InputController: publish semantic action messages from raw keyboard state"
```

(Skip this step if git isn't set up in this environment yet — leave the changes uncommitted.)

---

## Task 2: Migrate `player_input` to consume action messages

**Files:**
- Modify: `src/engine/systems.rs`
- Modify: `src/engine/mod.rs`

**Interfaces:**
- Consumes: `input::InputAction`, `input::ActionPressed`, `input::ActionReleased`, `input::InputSet` (Task 1).

- [ ] **Step 1: Write the failing tests**

Replace the two keyboard-based tests in `src/engine/systems.rs`'s `#[cfg(test)] mod tests` block (`player_input_uses_scripted_move_speed` and `player_input_uses_scripted_jump_force`) with message-based versions. Replace this:

```rust
    #[test]
    fn player_input_uses_scripted_move_speed() {
        let mut app = App::new();
        app.insert_resource(ScriptedTuning {
            move_speed: 250.0,
            jump_force: 500.0,
            gravity: 980.0,
        });
        let mut input = ButtonInput::<KeyCode>::default();
        input.press(KeyCode::KeyD);
        app.insert_resource(input);
        let entity = app
            .world_mut()
            .spawn((Player, Velocity::default(), Grounded(false)))
            .id();
        app.add_systems(Update, player_input);

        app.update();

        let velocity = app.world().get::<Velocity>(entity).unwrap();
        assert_eq!(velocity.0.x, 250.0);
    }

    #[test]
    fn player_input_uses_scripted_jump_force() {
        let mut app = App::new();
        app.insert_resource(ScriptedTuning {
            move_speed: 200.0,
            jump_force: 650.0,
            gravity: 980.0,
        });
        let mut input = ButtonInput::<KeyCode>::default();
        input.press(KeyCode::Space);
        app.insert_resource(input);
        let entity = app
            .world_mut()
            .spawn((Player, Velocity::default(), Grounded(true)))
            .id();
        app.add_systems(Update, player_input);

        app.update();

        let velocity = app.world().get::<Velocity>(entity).unwrap();
        assert_eq!(velocity.0.y, 650.0);
    }
```

with this:

```rust
    #[test]
    fn player_input_uses_scripted_move_speed() {
        let mut app = App::new();
        app.add_message::<ActionPressed>();
        app.add_message::<ActionReleased>();
        app.insert_resource(ScriptedTuning {
            move_speed: 250.0,
            jump_force: 500.0,
            gravity: 980.0,
        });
        let entity = app
            .world_mut()
            .spawn((Player, Velocity::default(), Grounded(false)))
            .id();
        app.add_systems(Update, player_input);
        app.world_mut()
            .write_message(ActionPressed(InputAction::MoveRight));

        app.update();

        let velocity = app.world().get::<Velocity>(entity).unwrap();
        assert_eq!(velocity.0.x, 250.0);
    }

    #[test]
    fn player_input_uses_scripted_jump_force() {
        let mut app = App::new();
        app.add_message::<ActionPressed>();
        app.add_message::<ActionReleased>();
        app.insert_resource(ScriptedTuning {
            move_speed: 200.0,
            jump_force: 650.0,
            gravity: 980.0,
        });
        let entity = app
            .world_mut()
            .spawn((Player, Velocity::default(), Grounded(true)))
            .id();
        app.add_systems(Update, player_input);
        app.world_mut().write_message(ActionPressed(InputAction::Jump));

        app.update();

        let velocity = app.world().get::<Velocity>(entity).unwrap();
        assert_eq!(velocity.0.y, 650.0);
    }
```

Add `use crate::engine::input::{ActionPressed, ActionReleased, InputAction};` to the imports at the top of the `tests` module (alongside the existing `use super::*; use crate::engine::scripting::ScriptedTuning;`).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib engine::systems`
Expected: FAIL to compile — `player_input`'s current signature takes `Res<ButtonInput<KeyCode>>`, not message readers, so `ActionPressed`/`ActionReleased`/`InputAction` aren't meaningful to it yet, and the old `ButtonInput`-pressing code is gone from the test.

- [ ] **Step 3: Refactor `player_input`**

Replace the current `player_input` function in `src/engine/systems.rs`:

```rust
pub fn player_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    tuning: Res<ScriptedTuning>,
    mut query: Query<(&mut Velocity, &Grounded), With<Player>>,
) {
    let Ok((mut velocity, grounded)) = query.single_mut() else {
        return;
    };

    let mut dir = 0.0;
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        dir -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        dir += 1.0;
    }
    velocity.0.x = dir * tuning.move_speed;

    if keyboard.just_pressed(KeyCode::Space) && grounded.0 {
        velocity.0.y = tuning.jump_force;
    }
}
```

with:

```rust
#[derive(Default)]
pub struct MoveDirState {
    left: bool,
    right: bool,
}

pub fn player_input(
    mut pressed_events: MessageReader<ActionPressed>,
    mut released_events: MessageReader<ActionReleased>,
    mut move_state: Local<MoveDirState>,
    tuning: Res<ScriptedTuning>,
    mut query: Query<(&mut Velocity, &Grounded), With<Player>>,
) {
    let mut jump_requested = false;
    for ActionPressed(action) in pressed_events.read() {
        match action {
            InputAction::MoveLeft => move_state.left = true,
            InputAction::MoveRight => move_state.right = true,
            InputAction::Jump => jump_requested = true,
            _ => {}
        }
    }
    for ActionReleased(action) in released_events.read() {
        match action {
            InputAction::MoveLeft => move_state.left = false,
            InputAction::MoveRight => move_state.right = false,
            _ => {}
        }
    }

    let Ok((mut velocity, grounded)) = query.single_mut() else {
        return;
    };

    let mut dir = 0.0;
    if move_state.left {
        dir -= 1.0;
    }
    if move_state.right {
        dir += 1.0;
    }
    velocity.0.x = dir * tuning.move_speed;

    if jump_requested && grounded.0 {
        velocity.0.y = tuning.jump_force;
    }
}
```

Note `jump_requested` is a plain local variable, not persisted state — this deliberately matches the old `just_pressed` semantics: a jump press is evaluated once, in the frame it happens, and dropped if the player isn't grounded at that instant (it does not queue up and fire later). Only `MoveLeft`/`MoveRight` need cross-frame persistence (`Local<MoveDirState>`), since they're held-state, not edge-triggered.

Add `use crate::engine::input::{ActionPressed, ActionReleased, InputAction};` to the top of `src/engine/systems.rs`, alongside the existing `use crate::engine::components::*;` and `use crate::engine::scripting::ScriptedTuning;`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib engine::systems`
Expected: PASS (both rewritten tests, plus the untouched `apply_gravity_uses_scripted_gravity`).

- [ ] **Step 5: Order `publish_input_actions` before `player_input`'s chain**

In `src/engine/mod.rs`, find the `Update` system registration containing `systems::player_input` (currently chained with `apply_gravity`, `ground_detection`, `apply_velocity`, `camera_follow`) and append `.after(input::InputSet)` to the whole chain:

```rust
.add_systems(
    Update,
    (
        systems::player_input,
        systems::apply_gravity,
        systems::ground_detection,
        systems::apply_velocity,
        systems::camera_follow,
    )
        .chain()
        .after(input::InputSet),
)
```

- [ ] **Step 6: Build and run the full suite**

Run: `cargo build`
Expected: succeeds.

Run: `cargo test`
Expected: all pass (7 tests now: the 2 from Task 1, the 2 rewritten here, plus `apply_gravity_uses_scripted_gravity` and the 2 in `scripting.rs`).

- [ ] **Step 7: Manual QA — movement and jump still feel identical**

Run: `cargo run`. Confirm: A/D and arrow keys move the player left/right at the same speed as before; Space jumps only while grounded; holding both A and ArrowLeft then releasing just one still lets movement continue (this is the dedup behavior from Task 1, now exercised through real gameplay).

- [ ] **Step 8: Commit**

```bash
git add src/engine/systems.rs src/engine/mod.rs
git commit -m "Migrate player_input to subscribe to InputController action messages"
```

(Skip if git isn't set up yet.)

---

## Task 3: Migrate `toggle_ui_visibility` to consume action messages

**Files:**
- Modify: `src/engine/debug_ui.rs`
- Modify: `src/engine/mod.rs`

**Interfaces:**
- Consumes: `input::InputAction`, `input::ActionPressed`, `input::InputSet` (Task 1).

- [ ] **Step 1: Write the failing test**

Add this test to `src/engine/debug_ui.rs` (create a `#[cfg(test)] mod tests` block at the bottom of the file if one doesn't exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::input::{ActionPressed, InputAction};

    #[test]
    fn toggle_ui_visibility_flips_on_toggle_action() {
        let mut app = App::new();
        app.add_message::<ActionPressed>();
        app.insert_resource(UiVisible(false));
        app.add_systems(Update, toggle_ui_visibility);
        app.world_mut()
            .write_message(ActionPressed(InputAction::ToggleDebugUi));

        app.update();

        assert!(app.world().resource::<UiVisible>().0);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib engine::debug_ui`
Expected: FAIL to compile — `toggle_ui_visibility` currently takes `Res<ButtonInput<KeyCode>>`, and nothing in the file imports `ActionPressed`/`InputAction` yet.

- [ ] **Step 3: Refactor `toggle_ui_visibility`**

Replace:

```rust
pub fn toggle_ui_visibility(keyboard: Res<ButtonInput<KeyCode>>, mut visible: ResMut<UiVisible>) {
    if keyboard.just_pressed(KeyCode::F1) {
        visible.0 = !visible.0;
    }
}
```

with:

```rust
pub fn toggle_ui_visibility(
    mut pressed_events: MessageReader<ActionPressed>,
    mut visible: ResMut<UiVisible>,
) {
    for ActionPressed(action) in pressed_events.read() {
        if *action == InputAction::ToggleDebugUi {
            visible.0 = !visible.0;
        }
    }
}
```

Add `use crate::engine::input::{ActionPressed, InputAction};` to the top of `src/engine/debug_ui.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib engine::debug_ui`
Expected: PASS.

- [ ] **Step 5: Order `publish_input_actions` before this system**

In `src/engine/mod.rs`, find the `Update` registration containing `debug_ui::toggle_ui_visibility` (currently paired with `debug_ui::apply_level_reload`) and append `.after(input::InputSet)`:

```rust
.add_systems(
    Update,
    (debug_ui::toggle_ui_visibility, debug_ui::apply_level_reload).after(input::InputSet),
)
```

- [ ] **Step 6: Build and run the full suite**

Run: `cargo build`
Expected: succeeds.

Run: `cargo test`
Expected: all pass.

- [ ] **Step 7: Manual QA**

Run: `cargo run`, press F1, confirm the Debug window / menu bar / asset panel / status bar still all appear and disappear together exactly as before.

- [ ] **Step 8: Commit**

```bash
git add src/engine/debug_ui.rs src/engine/mod.rs
git commit -m "Migrate toggle_ui_visibility to subscribe to InputController action messages"
```

(Skip if git isn't set up yet.)

---

## Task 4: Migrate `request_tuning_validation` to consume action messages

**Files:**
- Modify: `src/engine/scripting.rs`
- Modify: `src/engine/mod.rs`

**Interfaces:**
- Consumes: `input::InputAction`, `input::ActionPressed`, `input::InputSet` (Task 1).

- [ ] **Step 1: Write the failing test**

Add this test to `src/engine/scripting.rs`'s existing `#[cfg(test)] mod tuning_validation_tests` block:

```rust
    #[test]
    fn request_tuning_validation_sends_callback_on_validate_action() {
        use crate::engine::input::{ActionPressed, InputAction};
        use bevy_mod_scripting::prelude::ScriptAsset;
        use bevy_mod_scripting_core::event::CallbackLabel;

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
        app.add_message::<ActionPressed>();
        app.add_message::<ScriptCallbackEvent>();
        app.insert_resource(CapturedCallbacks::default());

        let mut script_assets = Assets::<ScriptAsset>::default();
        let handle = script_assets.add(ScriptAsset::new(String::new()));
        app.insert_resource(script_assets);
        app.world_mut()
            .spawn((ScriptComponent(vec![handle]), PlayerTuningScript));

        app.add_systems(
            Update,
            (request_tuning_validation, capture_callbacks).chain(),
        );
        app.world_mut()
            .write_message(ActionPressed(InputAction::ValidateTuning));

        app.update();

        let captured = app.world().resource::<CapturedCallbacks>();
        assert_eq!(captured.0, vec![OnGetTuning.into()]);
    }
```

This test exercises `request_tuning_validation`'s own request-construction logic in isolation — it doesn't need `BMSPlugin` or a real Lua script, since it only checks that the right `ScriptCallbackEvent` gets written, not that Lua actually runs. That wasn't possible before this refactor (the old version's trigger condition was buried in a direct keyboard read tightly coupled to a real `App::run()` loop); decoupling the trigger from raw input is what makes this newly testable.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib engine::scripting`
Expected: FAIL to compile — `request_tuning_validation` currently takes `Res<ButtonInput<KeyCode>>`.

- [ ] **Step 3: Refactor `request_tuning_validation`**

Replace:

```rust
pub fn request_tuning_validation(
    keyboard: Res<ButtonInput<KeyCode>>,
    query: Query<(Entity, &ScriptComponent), With<PlayerTuningScript>>,
    mut callbacks: MessageWriter<ScriptCallbackEvent>,
) {
    if !keyboard.just_pressed(KeyCode::F5) {
        return;
    }
    let Ok((entity, script)) = query.single() else {
        warn!("F5 pressed but no player tuning script is loaded yet");
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

with:

```rust
pub fn request_tuning_validation(
    mut pressed_events: MessageReader<ActionPressed>,
    query: Query<(Entity, &ScriptComponent), With<PlayerTuningScript>>,
    mut callbacks: MessageWriter<ScriptCallbackEvent>,
) {
    let triggered = pressed_events
        .read()
        .any(|ActionPressed(action)| *action == InputAction::ValidateTuning);
    if !triggered {
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

Add `use crate::engine::input::{ActionPressed, InputAction};` to the top of `src/engine/scripting.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib engine::scripting`
Expected: PASS.

- [ ] **Step 5: Order `publish_input_actions` before this system's group**

In `src/engine/mod.rs`, find the `Update` registration containing `scripting::request_tuning_validation` (currently grouped with `scripting::handle_tuning_validation_response` and `event_handler::<scripting::OnGetTuning, LuaScriptingPlugin>`) and append `.after(input::InputSet)`:

```rust
.add_systems(
    Update,
    (
        scripting::request_tuning_validation,
        scripting::handle_tuning_validation_response,
        event_handler::<scripting::OnGetTuning, LuaScriptingPlugin>,
    )
        .after(input::InputSet),
)
```

- [ ] **Step 6: Build and run the full suite**

Run: `cargo build`
Expected: succeeds.

Run: `cargo test`
Expected: all pass (9 tests total now).

- [ ] **Step 7: Manual QA**

Run: `cargo run`, wait for the script to load, press F5, confirm the same `info!`/`warn!` match/mismatch log behavior from the earlier scripting sub-project still works.

- [ ] **Step 8: Commit**

```bash
git add src/engine/scripting.rs src/engine/mod.rs
git commit -m "Migrate request_tuning_validation to subscribe to InputController action messages"
```

(Skip if git isn't set up yet.)

---

## Task 5: Integration verification

No code changes — this task exists to verify the fully-migrated input pipeline end-to-end, since Tasks 2–4 each verified their own consumer in isolation but not all three together under real, continuous play.

**Files:** none.

- [ ] **Step 1: Full test suite**

Run: `cargo test`
Expected: all 9 tests pass.

- [ ] **Step 2: Confirm no direct `ButtonInput` reads remain outside `input.rs`**

Run: `grep -rn "ButtonInput" src/engine/systems.rs src/engine/debug_ui.rs src/engine/scripting.rs`
Expected: no matches — `input.rs` is now the only file reading `ButtonInput<KeyCode>` directly.

- [ ] **Step 3: Manual QA — full session**

Run: `cargo run` and, in one sitting:
1. Move left/right with both A/D and arrow keys, individually and combined (e.g., hold A and ArrowLeft together, release one, confirm movement doesn't stop until both are released).
2. Jump — confirm it only works while grounded, and holding Space doesn't repeat-fire mid-air.
3. Press F1 — confirm the Debug window, menu bar, asset sidebar, and status bar all appear/disappear together.
4. Press F5 — confirm the tuning validation log (`info!`/`warn!`) still fires correctly.
5. Use the "Reload Level" and "Exit Game" menu buttons — confirm both still work (these are unaffected by this refactor, but worth a sanity check since `mod.rs` changed).

- [ ] **Step 4: Commit** (only if Steps 1–3 all pass and there's nothing left uncommitted from earlier tasks)

```bash
git status
```

If there are uncommitted changes from a skipped commit step (no git identity was set up earlier), this is the point to either configure git identity and commit everything, or leave it for the user — do not force a commit without one of those two being true.
