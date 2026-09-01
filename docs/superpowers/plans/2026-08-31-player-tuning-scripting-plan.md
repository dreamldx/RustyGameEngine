# Player Tuning Scripting (Lua) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the player's `move_speed`, `jump_force`, and `gravity` live-editable via a Lua script (`assets/scripts/player.lua`), hot-reloading on save with no app restart, and add a debug round-trip check (`F5`) that queries the script directly to confirm it agrees with the live values — proving both the Lua→Rust and Rust→Lua call paths before any future sub-project (enemy AI, level triggers) depends on them.

**Architecture:** Add `bevy_mod_scripting` (Lua backend) and Bevy's `file_watcher` feature. Introduce a `ScriptedTuning` resource that `player_input`/`apply_gravity` read instead of hardcoded consts/components. A `Startup` system scans `assets/scripts/` for every `.lua` file, loads it, and spawns a `ScriptComponent` entity per file (marking the `player.lua` entity `PlayerTuningScript`). `player.lua`'s built-in `on_script_loaded` callback pushes values into `ScriptedTuning` via a Rust function bound onto Lua's `world` table. Pressing `F5` sends an async callback request to the script and a response-handling system compares the reply to `ScriptedTuning`.

**Tech Stack:** Rust, Bevy `0.19`, `bevy_mod_scripting` `0.21` (Lua 5.4 backend, feature `lua54`).

**Spec:** `docs/superpowers/specs/2026-08-31-player-tuning-scripting-design.md`

## Global Constraints

- `bevy_mod_scripting = "0.21"` with feature `["lua54"]` — this crate version targets Bevy `0.19` (its version number does not track Bevy's).
- `bevy` gains the `file_watcher` feature (required for hot reload; without it, edited scripts are only picked up on restart).
- Default tuning values, used both as `ScriptedTuning`'s `Default` and as `player.lua`'s initial globals: `move_speed = 200.0`, `jump_force = 500.0`, `gravity = 980.0`.
- Scripts live in `assets/scripts/`; loading must be directory-driven (scan for `.lua` files), never hardcoded to a single filename.
- A load/parse failure in one script must not prevent other scripts in the directory from loading.
- No automated tests for behavior that depends on BMS's asset pipeline (per spec, this isn't practically fakeable) — those are verified by the manual QA steps embedded in Tasks 3 and 4. Pure Rust logic (the tuning refactor in Task 2, the comparison logic in Task 4) gets real unit tests.

**Known API risk:** `bevy_mod_scripting`'s function-binding and callback-event API (Tasks 3–4) was verified against the crate's own published examples (`examples/run_script.rs`, `examples/script_loading.rs`, `examples/custom_conversions.rs` in the `makspll/bevy_mod_scripting` repo) as of this plan's writing, but the crate moves fast and some exact symbol names — particularly the callback-response event type in Task 4 — could not be confirmed from examples alone. Where a step says "verify against `cargo doc`," treat that as a required step, not optional polish: run it, and adjust the named type/method to what's actually in the locally resolved crate version before considering the step done.

---

## Task 1: Add scripting dependencies

**Files:**
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `bevy_mod_scripting` crate available with Lua 5.4 backend; `bevy`'s asset system watches `assets/` for file changes.

- [ ] **Step 1: Add the dependencies**

Edit `Cargo.toml`:

```toml
[dependencies]
bevy = { version = "0.19", features = ["file_watcher"] }
bevy_mod_scripting = { version = "0.21", features = ["lua54"] }
```

- [ ] **Step 2: Verify it resolves and compiles**

Run: `cargo check`
Expected: succeeds (no source code uses the new crate yet, so this only proves the dependency graph resolves and the crate itself compiles for this platform/toolchain — first run will take a while as it's a large dependency tree).

If `cargo check` fails on `bevy_mod_scripting` itself (not a version-resolution error), the `0.21`/`lua54` combination may need adjusting — check `https://crates.io/crates/bevy_mod_scripting/versions` for the latest patch under `0.21` and use that instead.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "Add bevy_mod_scripting (Lua) and enable asset hot reload"
```

---

## Task 2: Scripted tuning resource (no scripting yet)

Replaces the hardcoded `MoveSpeed`/`JumpForce` components and the `GRAVITY` const with a single `ScriptedTuning` resource, defaulted to today's values. This is a pure Bevy/Rust refactor — it doesn't touch `bevy_mod_scripting` at all, and proves the game behaves identically before any script exists (spec's manual test 1 depends on this being correct first).

**Files:**
- Create: `src/engine/scripting.rs`
- Modify: `src/engine/mod.rs`
- Modify: `src/engine/components.rs`
- Modify: `src/engine/player.rs:7-21`
- Modify: `src/engine/systems.rs`

**Interfaces:**
- Produces: `scripting::ScriptedTuning { move_speed: f32, jump_force: f32, gravity: f32 }` (a `Resource`, `Default` = `200.0`/`500.0`/`980.0`). Consumed by `systems::player_input` and `systems::apply_gravity`, and (Task 3) written by the scripting push path.

- [ ] **Step 1: Create the resource**

Create `src/engine/scripting.rs`:

```rust
use bevy::prelude::*;

#[derive(Resource)]
pub struct ScriptedTuning {
    pub move_speed: f32,
    pub jump_force: f32,
    pub gravity: f32,
}

impl Default for ScriptedTuning {
    fn default() -> Self {
        Self {
            move_speed: 200.0,
            jump_force: 500.0,
            gravity: 980.0,
        }
    }
}
```

- [ ] **Step 2: Write the failing tests**

Add to the bottom of `src/engine/systems.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scripting::ScriptedTuning;

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

    #[test]
    fn apply_gravity_uses_scripted_gravity() {
        use bevy::time::TimeUpdateStrategy;
        use std::time::Duration;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            0.5,
        )));
        app.insert_resource(ScriptedTuning {
            move_speed: 200.0,
            jump_force: 500.0,
            gravity: 1000.0,
        });
        let entity = app.world_mut().spawn((Player, Velocity::default())).id();
        app.add_systems(Update, apply_gravity);

        app.update();

        let velocity = app.world().get::<Velocity>(entity).unwrap();
        assert_eq!(velocity.0.y, -500.0);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test`
Expected: FAIL to compile — `player_input` and `apply_gravity` don't yet take a `ScriptedTuning` resource, and `MoveSpeed`/`JumpForce` are still required components on the test entities that aren't being inserted.

- [ ] **Step 4: Refactor `components.rs`**

In `src/engine/components.rs`, delete the `MoveSpeed` and `JumpForce` component definitions and their `Default` impls entirely (lines 15-19 and 30-34 in the current file — the `JumpForce`/`MoveSpeed` structs and their `impl Default` blocks). Keep `Player`, `Platform`, `Velocity`, `Grounded`, `PlayerSpawn`, and `Grounded`'s `Default` impl unchanged.

- [ ] **Step 5: Refactor `player.rs`**

In `src/engine/player.rs`, `spawn_player` currently inserts `JumpForce::default()` and `MoveSpeed::default()` — remove both from the spawn tuple:

```rust
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
    ));
}
```

- [ ] **Step 6: Refactor `systems.rs`**

Replace the `GRAVITY` const and update `player_input`/`apply_gravity` to read `ScriptedTuning`:

```rust
use crate::engine::components::*;
use crate::engine::scripting::ScriptedTuning;
use bevy::prelude::*;

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

pub fn apply_gravity(
    time: Res<Time>,
    tuning: Res<ScriptedTuning>,
    mut query: Query<&mut Velocity, With<Player>>,
) {
    for mut velocity in &mut query {
        velocity.0.y -= tuning.gravity * time.delta_secs();
    }
}
```

Leave `apply_velocity`, `ground_detection`, and `camera_follow` unchanged.

- [ ] **Step 7: Register the resource and confirm the module compiles**

In `src/engine/mod.rs`, add the module and initialize the resource:

```rust
pub mod components;
pub mod level;
pub mod player;
pub mod scripting;
pub mod systems;

use bevy::prelude::*;
use scripting::ScriptedTuning;

pub struct EnginePlugin;

impl Plugin for EnginePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptedTuning>()
            .add_systems(
                Startup,
                (
                    level::spawn_level,
                    player::spawn_player,
                    player::spawn_camera,
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
            );
    }
}
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS (3 tests: `player_input_uses_scripted_move_speed`, `player_input_uses_scripted_jump_force`, `apply_gravity_uses_scripted_gravity`).

- [ ] **Step 9: Manual sanity check**

Run: `cargo run`
Expected: game behaves exactly as before this task — same move speed, jump height, fall speed (`200.0`/`500.0`/`980.0`, now sourced from `ScriptedTuning`'s `Default` instead of the old components/const).

- [ ] **Step 10: Commit**

```bash
git add src/engine/scripting.rs src/engine/mod.rs src/engine/components.rs src/engine/player.rs src/engine/systems.rs
git commit -m "Read player tuning from a ScriptedTuning resource instead of hardcoded values"
```

---

## Task 3: Lua push path — directory-scanned scripts write ScriptedTuning

Wires up `bevy_mod_scripting`: scans `assets/scripts/` for every `.lua` file, loads and attaches each as a script entity, and has `player.lua`'s `on_script_loaded` callback push values into `ScriptedTuning` via a Rust function callable from Lua.

**Files:**
- Create: `assets/scripts/player.lua`
- Modify: `src/engine/scripting.rs`
- Modify: `src/engine/mod.rs`

**Interfaces:**
- Consumes: `scripting::ScriptedTuning` (Task 2).
- Produces: `scripting::PlayerTuningScript` (marker `Component`, inserted on the entity loaded from `player.lua`) — consumed by Task 4's `F5` validation system.

- [ ] **Step 1: Write the Lua script**

Create `assets/scripts/player.lua`:

```lua
move_speed = 200.0
jump_force = 500.0
gravity = 980.0

function on_script_loaded()
    world.set_player_tuning(move_speed, jump_force, gravity)
end
```

- [ ] **Step 2: Add the marker component, the Lua-callable function, and the directory scan**

Append to `src/engine/scripting.rs`:

```rust
use bevy_mod_scripting::prelude::*;
use std::fs;
use std::path::Path;

#[derive(Component)]
pub struct PlayerTuningScript;

#[script_bindings(remote, unregistered)]
impl World {
    pub fn set_player_tuning(&mut self, move_speed: f32, jump_force: f32, gravity: f32) {
        if let Some(mut tuning) = self.get_resource_mut::<ScriptedTuning>() {
            tuning.move_speed = move_speed;
            tuning.jump_force = jump_force;
            tuning.gravity = gravity;
        }
    }
}

pub fn load_scripts(asset_server: Res<AssetServer>, mut commands: Commands) {
    let dir = Path::new("assets/scripts");
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            error!("Failed to read scripts directory {}: {}", dir.display(), e);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lua") {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|f| f.to_str()) else {
            continue;
        };

        let handle = asset_server.load::<ScriptAsset>(format!("scripts/{file_name}"));
        let mut entity = commands.spawn(ScriptComponent(vec![handle]));
        if file_name == "player.lua" {
            entity.insert(PlayerTuningScript);
        }
    }
}
```

`register_functions` is generated by the `#[script_bindings]` macro above (matching the pattern in `bevy_mod_scripting`'s `examples/custom_conversions.rs`) — it doesn't need to be written by hand, but **verify this**: run `cargo doc -p bevy_mod_scripting_bindings --open` (or search the crate's docs) after Step 4 fails to compile, and confirm the generated registration function's exact name and signature; adjust the call in Step 3 below if it differs.

- [ ] **Step 3: Wire the plugin and systems into `EnginePlugin`**

In `src/engine/mod.rs`:

```rust
pub mod components;
pub mod level;
pub mod player;
pub mod scripting;
pub mod systems;

use bevy::prelude::*;
use bevy_mod_scripting::prelude::*;
use scripting::ScriptedTuning;

pub struct EnginePlugin;

impl Plugin for EnginePlugin {
    fn build(&self, app: &mut App) {
        scripting::register_functions(app.world_mut());

        app.add_plugins(BMSPlugin)
            .init_resource::<ScriptedTuning>()
            .add_systems(
                Startup,
                (
                    level::spawn_level,
                    player::spawn_player,
                    player::spawn_camera,
                    scripting::load_scripts,
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
            );
    }
}
```

Note `scripting::load_scripts` is chained after `player::spawn_camera` — it doesn't depend on the others, but keeping all `Startup` work in one deterministic chain avoids introducing ordering ambiguity.

- [ ] **Step 4: Build and fix against the real crate API**

Run: `cargo build`

This is very likely to fail on the first attempt — `bevy_mod_scripting`'s exact macro-generated names/signatures could not be fully confirmed from examples alone (see "Known API risk" above). For each compile error:
1. Read the error — it will name the missing/mismatched symbol.
2. Run `cargo doc -p bevy_mod_scripting --open` (add `-p bevy_mod_scripting_bindings` or `-p bevy_mod_scripting_core` if the symbol lives in a sub-crate) and search for the correct name/signature.
3. Also check the locally-fetched crate source for more examples: `%USERPROFILE%\.cargo\registry\src\*\bevy_mod_scripting-0.21.*\examples\`.
4. Adjust `scripting.rs`/`mod.rs` and rebuild.

Expected once fixed: `cargo build` succeeds.

- [ ] **Step 5: Manual QA — behavior unchanged**

Run: `cargo run`
Expected: player movement, jump height, and fall speed feel identical to before this task (same `200.0`/`500.0`/`980.0` defaults — now arriving via the script instead of `ScriptedTuning`'s `Default`). Check the log for any BMS load errors.

- [ ] **Step 6: Manual QA — hot reload**

While `cargo run` is still running, edit `assets/scripts/player.lua` — change `move_speed = 200.0` to `move_speed = 400.0` — and save.
Expected: within a frame or two, holding the move keys makes the player noticeably faster, with no restart.

Revert the edit back to `200.0` afterward.

- [ ] **Step 7: Manual QA — bad script doesn't crash the game**

While running, introduce a syntax error into `player.lua` (e.g. delete the `end` on `on_script_loaded`), save.
Expected: an error is logged, and the game keeps running with the last-good tuning values (movement/jump/gravity keep working as before the bad edit).

Fix the syntax error afterward so the file is valid again.

- [ ] **Step 8: Manual QA — directory scan is generic**

Create a second file `assets/scripts/_debug_noop.lua` containing just:

```lua
function on_script_loaded()
end
```

Restart with `cargo run`.
Expected: no errors logged for the second script, and player tuning is unaffected — proving `load_scripts` loads whatever `.lua` files are present rather than a hardcoded filename.

Delete `assets/scripts/_debug_noop.lua` afterward — it isn't part of this task's deliverable.

- [ ] **Step 9: Commit**

```bash
git add assets/scripts/player.lua src/engine/scripting.rs src/engine/mod.rs Cargo.lock
git commit -m "Load player.lua via bevy_mod_scripting and push tuning into ScriptedTuning on (re)load"
```

---

## Task 4: Lua pull path — F5 validates the script against ScriptedTuning

Adds a `get_tuning()` Lua function and a debug `F5` keybinding that requests it, then compares the response against `ScriptedTuning`, proving Rust can call into Lua on demand (the same mechanism the future enemy-AI sub-project will use for per-frame `on_update` calls).

**Files:**
- Modify: `assets/scripts/player.lua`
- Modify: `src/engine/scripting.rs`
- Modify: `src/engine/mod.rs`

**Interfaces:**
- Consumes: `scripting::ScriptedTuning`, `scripting::PlayerTuningScript` (Tasks 2–3).

- [ ] **Step 1: Add `get_tuning()` to the script**

In `assets/scripts/player.lua`, add alongside the existing `on_script_loaded`:

```lua
function get_tuning()
    return move_speed, jump_force, gravity
end
```

- [ ] **Step 2: Write the failing tests for the comparison logic**

Add to `src/engine/scripting.rs` (this part is pure Rust — no BMS involved — so it's fully unit-testable):

```rust
pub fn describe_tuning_mismatch(current: &ScriptedTuning, queried: (f32, f32, f32)) -> Option<String> {
    let (move_speed, jump_force, gravity) = queried;
    let mut mismatches = Vec::new();
    if (current.move_speed - move_speed).abs() > f32::EPSILON {
        mismatches.push(format!(
            "move_speed: resource={} script={}",
            current.move_speed, move_speed
        ));
    }
    if (current.jump_force - jump_force).abs() > f32::EPSILON {
        mismatches.push(format!(
            "jump_force: resource={} script={}",
            current.jump_force, jump_force
        ));
    }
    if (current.gravity - gravity).abs() > f32::EPSILON {
        mismatches.push(format!(
            "gravity: resource={} script={}",
            current.gravity, gravity
        ));
    }
    if mismatches.is_empty() {
        None
    } else {
        Some(mismatches.join(", "))
    }
}

#[cfg(test)]
mod tuning_validation_tests {
    use super::*;

    #[test]
    fn describe_tuning_mismatch_returns_none_when_equal() {
        let tuning = ScriptedTuning {
            move_speed: 200.0,
            jump_force: 500.0,
            gravity: 980.0,
        };
        assert_eq!(
            describe_tuning_mismatch(&tuning, (200.0, 500.0, 980.0)),
            None
        );
    }

    #[test]
    fn describe_tuning_mismatch_reports_differing_fields() {
        let tuning = ScriptedTuning {
            move_speed: 200.0,
            jump_force: 500.0,
            gravity: 980.0,
        };
        let msg = describe_tuning_mismatch(&tuning, (250.0, 500.0, 1000.0))
            .expect("expected a mismatch description");
        assert!(msg.contains("move_speed"));
        assert!(msg.contains("gravity"));
        assert!(!msg.contains("jump_force"));
    }
}
```

Since `describe_tuning_mismatch` doesn't exist until Step 2's function body above is added, write the test module first (or in the same edit) and run tests to confirm they fail before it exists — for a same-file addition like this, it's acceptable to add the function and its tests together in one edit; the important check is Step 3 below actually exercises both.

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: PASS (`describe_tuning_mismatch_returns_none_when_equal`, `describe_tuning_mismatch_reports_differing_fields`), alongside the three from Task 2 still passing.

- [ ] **Step 4: Add the callback label and the request system**

Append to `src/engine/scripting.rs`:

```rust
callback_labels!(OnGetTuning => "get_tuning");

pub fn request_tuning_validation(
    keyboard: Res<ButtonInput<KeyCode>>,
    query: Query<&ScriptComponent, With<PlayerTuningScript>>,
    mut callbacks: MessageWriter<ScriptCallbackEvent>,
) {
    if !keyboard.just_pressed(KeyCode::F5) {
        return;
    }
    let Ok(script) = query.single() else {
        warn!("F5 pressed but no player tuning script is loaded yet");
        return;
    };
    let Some(handle) = script.0.first() else {
        return;
    };
    callbacks.write(
        ScriptCallbackEvent::new_for_static_script(OnGetTuning, vec![], handle.clone())
            .with_response(),
    );
}
```

- [ ] **Step 5: Add the response-handling system — verify the response type first**

`bevy_mod_scripting`'s exact response-event type for a `.with_response()` callback request could not be confirmed from the examples fetched while writing this plan. Before writing this system:

1. Run `cargo doc -p bevy_mod_scripting_core --open` (try `bevy_mod_scripting_bindings` too if not found there) and search for a type with "Response" or "Callback" in the name that pairs with `ScriptCallbackEvent`.
2. Confirm how to read the returned Lua values back out (likely a `ScriptValue`, the same enum seen in `examples/custom_conversions.rs` — check whether multiple Lua return values arrive as `ScriptValue::List(Vec<ScriptValue>)` or some other shape).

Once confirmed, append a system to `src/engine/scripting.rs` shaped like this (adjust the event type, field names, and `ScriptValue` extraction to match what Step 5.1–5.2 found — this is the concrete starting point, not a guess to ship unmodified):

```rust
pub fn handle_tuning_validation_response(
    tuning: Res<ScriptedTuning>,
    mut responses: MessageReader<ScriptCallbackResponseEvent>, // verify this type name
) {
    for response in responses.read() {
        if response.label != OnGetTuning.into() {
            continue;
        }
        // Extract three f32s from `response.result` (a ScriptValue or similar) —
        // exact extraction depends on what Step 5.1-5.2 found. Then:
        let queried = /* (move_speed, jump_force, gravity) extracted above */;
        match describe_tuning_mismatch(&tuning, queried) {
            None => info!("Scripted tuning matches ScriptedTuning resource"),
            Some(diff) => warn!("Scripted tuning mismatch: {diff}"),
        }
    }
}
```

- [ ] **Step 6: Register the new systems**

In `src/engine/mod.rs`, add both systems to `Update` (order relative to the existing gameplay chain doesn't matter — they're independent of it):

```rust
.add_systems(
    Update,
    (
        scripting::request_tuning_validation,
        scripting::handle_tuning_validation_response,
        event_handler::<scripting::OnGetTuning, LuaScriptingPlugin>,
    ),
)
```

`event_handler::<OnGetTuning, LuaScriptingPlugin>` is required for the `get_tuning` callback to actually be dispatched to the script — this mirrors `examples/custom_conversions.rs`'s `event_handler::<OnExample, LuaScriptingPlugin>` registration for its custom callback label. (`on_script_loaded` in Task 3 didn't need this because it's a built-in lifecycle callback BMS dispatches automatically; `get_tuning` is a custom label, so it does.)

- [ ] **Step 7: Build**

Run: `cargo build`
Expected: succeeds. If `event_handler` or `ScriptCallbackEvent` aren't found at the paths used above, check `bevy_mod_scripting::prelude::*` vs. `bevy_mod_scripting_core`/`bevy_mod_scripting_bindings` re-exports via `cargo doc`.

- [ ] **Step 8: Manual QA — match case**

Run: `cargo run`, wait for the script to load, press `F5`.
Expected: an `info!` log confirming the scripted tuning matches `ScriptedTuning`.

- [ ] **Step 9: Manual QA — mismatch case**

While running, edit `assets/scripts/player.lua` so `get_tuning()` returns different numbers than the globals `on_script_loaded` reports — e.g.:

```lua
function get_tuning()
    return move_speed + 1, jump_force, gravity
end
```

Save, press `F5` again.
Expected: a `warn!` log naming `move_speed` as mismatched, with both the resource and script values shown.

Revert this temporary edit afterward.

- [ ] **Step 10: Commit**

```bash
git add assets/scripts/player.lua src/engine/scripting.rs src/engine/mod.rs
git commit -m "Add F5 debug validation of scripted tuning via a Lua get_tuning() round trip"
```
