# Player Tuning Scripting (Lua) — Design

## Context

MarioEngine is a 2D Bevy platformer (`bevy = "0.19"`). Gameplay tuning constants
(player move speed, jump force, gravity) are currently hardcoded in Rust
(`src/engine/components.rs` component `Default` impls, and a `GRAVITY` const in
`src/engine/systems.rs`). Changing any of them requires editing Rust and
recompiling.

This is sub-project 1 of a larger scripting effort. The long-term goal is
Lua-scripted enemy AI and level-file-driven event triggers, but those are out
of scope here — they depend on this same foundation (the scripting plugin,
asset loading, hot reload) and will be designed as separate specs once this
closes the loop end-to-end.

**Goal of this sub-project:** integrate `bevy_mod_scripting` with a Lua
backend, and use it for exactly one thing — making player tuning parameters
(`move_speed`, `jump_force`, `gravity`) live-editable via a Lua script file,
with changes taking effect without restarting the app (hot reload). This
proves the scripting pipeline works before building anything that depends on
it.

The pipeline is proven in both directions: Lua calls into Rust (script push
on load/reload) and Rust calls into Lua (on-demand query for validation).
The future enemy-AI sub-project needs Rust to call into Lua every frame
(`on_update`-style), so exercising the Rust→Lua call path here — even for a
trivial validation use case — is a low-risk way to confirm that mechanism
works before anything depends on it at 60fps.

## Compatibility

`bevy_mod_scripting` crate version `0.21.x` targets Bevy `0.19` (confirmed
against the crate's own `Cargo.toml`, which pins `bevy = "0.19"` — the
crate's version number does not track Bevy's minor version, despite an
earlier assumption to that effect). This project's existing
`bevy = "0.19"` dependency is compatible with `bevy_mod_scripting = "0.21"`.

## Approach

Three approaches were considered:

- **A. Config-pull** — script sets globals, Rust reads them once per
  load/reload into a resource. Simplest; no callback wiring.
- **B. Callback-driven (chosen)** — script defines an `on_script_loaded`
  callback that calls a Rust function exposed via `GlobalNamespace`, which
  writes the values into a resource. Slightly more moving parts than A, but
  matches the callback style the future enemy-AI sub-project will need
  anyway (per-entity `ScriptComponent` + labeled callbacks), so this
  sub-project also serves as a working reference for that pattern.
- **C. Per-frame execution** — script runs every frame. Rejected: adds
  per-frame Lua/FFI overhead for what is static configuration; no current
  requirement needs the script to react to live game state.

Both A and B have negligible runtime cost (Lua only executes on load/hot
reload, never in the per-frame hot path); C is the only option with a real
runtime cost, which is why it was rejected outright rather than compared on
performance.

## Architecture

**Dependencies** (`Cargo.toml`):
- `bevy_mod_scripting = { version = "0.21", features = ["lua54"] }`
- `bevy` gains the `file_watcher` feature (enables asset hot reload — without
  it, edited script files are not picked up until restart)

**New asset:** `assets/scripts/player.lua` — defines `move_speed`,
`jump_force`, `gravity`, an `on_script_loaded` callback function that passes
them to the Rust-registered setter, and a plain (non-callback) function
`get_tuning()` returning the same three values, callable on demand from
Rust.

**Script loading is directory-based, not hardcoded to `player.lua`.** A
`Startup` system scans `assets/scripts/` (via `std::fs::read_dir`, the same
direct-filesystem-access pattern `level.rs` already uses for
`assets/levels/level1.txt`, rather than an asset-index file) for every
`.lua` file, and for each one: loads it through the `AssetServer` to get a
`Handle`, then spawns an entity with `ScriptComponent(vec![handle])`. This
means any `.lua` file dropped into `assets/scripts/` is automatically
loaded and — once `file_watcher` is enabled — automatically hot-reloaded on
save, with no code change required. That generality is intentional: the
future enemy-AI and level-trigger sub-projects will add their own `.lua`
files to the same directory and get loading + hot reload for free from this
mechanism. It does not, by itself, make those future scripts *do*
anything — the callback wiring for `on_update`-style behavior is still out
of scope here (see Out of Scope).

Because loading is now generic, the entity for `player.lua` specifically
needs to be identifiable for the `F5` validation system (below). The
directory scan additionally inserts a `PlayerTuningScript` marker component
on the entity whose source filename is `player.lua`.

**New Rust pieces (in `src/engine/`):**
- `ScriptedTuning` resource: `{ move_speed: f32, jump_force: f32, gravity: f32 }`.
  Initialized with the current hardcoded defaults (`200.0`, `500.0`, `980.0`)
  so the game behaves identically before the script has loaded.
- A `set_player_tuning(move_speed, jump_force, gravity)` method exposed on
  the Lua `world` table via BMS's function-binding mechanism (called from
  Lua as `world.set_player_tuning(...)`). Writes into `ScriptedTuning`.
- A `Startup` system, `load_scripts`, that scans `assets/scripts/` and
  spawns one script entity per `.lua` file found (details above), marking
  the `player.lua` entity with `PlayerTuningScript`.
- A debug `Update` system, `validate_scripted_tuning`, gated on
  `keyboard.just_pressed(KeyCode::F5)`. BMS's script-callback mechanism is
  message-based rather than a direct synchronous call: pressing `F5` writes
  a `get_tuning` callback request (via `MessageWriter<ScriptCallbackEvent>`,
  requesting a response) targeting the `PlayerTuningScript` entity's script
  handle; a second system reads the response message on a later frame,
  compares the three returned values against the current `ScriptedTuning`
  resource, and logs the result: `info!` if they match, `warn!` (naming the
  mismatched field(s) and both values) if they don't.
- `EnginePlugin` registers the BMS Lua scripting plugin and the new startup
  and debug systems.

**Changed Rust pieces:**
- `player_input` (`systems.rs`) reads move speed / jump force from
  `ScriptedTuning` instead of the `MoveSpeed`/`JumpForce` components.
- `apply_gravity` (`systems.rs`) reads gravity from `ScriptedTuning` instead
  of the `GRAVITY` const.
- `spawn_player` (`player.rs`) stops inserting `MoveSpeed`/`JumpForce`
  components (superseded by the resource); `Velocity`/`Grounded` stay as
  components since they're per-entity runtime state, not tuning.
- `components.rs`: `MoveSpeed`/`JumpForce` component definitions and their
  `Default` impls are removed (no longer used once `spawn_player` stops
  inserting them).

## Data Flow

1. App starts → `load_scripts` scans `assets/scripts/`, finds `player.lua`
   (currently the only file there), loads it, spawns a script entity
   (marked `PlayerTuningScript`) with `ScriptComponent` → BMS executes it.
2. Script execution fires `on_script_loaded`, which calls
   `set_player_tuning(move_speed, jump_force, gravity)`.
3. The registered Rust function writes those three values into
   `ScriptedTuning`.
4. Every frame, `player_input` and `apply_gravity` read current values from
   `ScriptedTuning`.
5. Editing and saving `player.lua` while the app is running triggers the
   file watcher → BMS reloads the script → step 2 repeats → `ScriptedTuning`
   updates → next frame reflects the new values. No restart needed.

**Rust → Lua (validation path, independent of the flow above):**

6. At any point after the script has loaded, pressing `F5` sends a
   `get_tuning` callback request to the script; when the response arrives
   (a later frame), a handler compares it to `ScriptedTuning` and logs
   whether they agree.

## Error Handling

If `player.lua` fails to parse, or `on_script_loaded` isn't called (e.g. the
script omits it), or the values passed to `set_player_tuning` are the wrong
type: BMS/Lua reports the error to Bevy's log. `ScriptedTuning` is only ever
written by a successful `set_player_tuning` call, so a bad edit leaves the
last-good values in place rather than crashing or zeroing out tuning. The
game keeps running with whatever `ScriptedTuning` last held.

A load or parse failure in one `.lua` file must not stop `load_scripts`
from loading the rest of `assets/scripts/` — each file's load/spawn is
independent, so one broken script only affects the entity for that file.

## Testing

No automated test coverage is planned for this sub-project — the surface
being changed is asset loading + a resource + reading it in two systems,
and the meaningful verification is behavioral, not unit-testable without
faking BMS's asset pipeline. Manual verification instead:

1. Run the app; confirm player movement, jump height, and fall speed feel
   identical to before the change (same defaults: `200.0` / `500.0` /
   `980.0`).
2. While the app is running, edit one or more values in `player.lua`, save,
   and confirm the corresponding behavior changes on the next frame without
   restarting the process.
3. Introduce a deliberate syntax error in `player.lua`, save, confirm an
   error is logged and gameplay continues using the last-good values rather
   than crashing.
4. Press `F5` after the script has loaded; confirm an `info!` match log
   appears. Then temporarily edit `player.lua` so `get_tuning()` returns
   different numbers than the globals it reports via `on_script_loaded`,
   save, press `F5` again, and confirm the `warn!` mismatch path logs
   correctly. Revert the temporary edit afterward.
5. Drop a second, harmless `.lua` file (e.g. one with no callbacks at all)
   into `assets/scripts/`, restart, and confirm it loads without errors and
   without affecting player tuning — proving `load_scripts` is genuinely
   directory-driven rather than hardcoded to `player.lua`. Remove the file
   afterward (it isn't part of this sub-project's deliverable).

## Out of Scope

- Enemy AI scripting (separate future sub-project; needs per-entity
  `ScriptComponent` + `on_update`-style callbacks, not just config-on-load).
- Level event/trigger scripting (separate future sub-project; needs
  `level.rs` grid-parser changes to spawn triggers).
- Any Lua API surface beyond the three tuning values named above.
