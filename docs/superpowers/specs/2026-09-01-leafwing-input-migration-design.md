# Migrate InputController to leafwing_input_manager — Design

## Context

MarioEngine currently has a hand-rolled `InputController` (`src/engine/input.rs`, built in an earlier sub-project): an `InputAction` enum, `ActionPressed`/`ActionReleased` Bevy messages, a fixed key-binding table, and a `publish_input_actions` system that aggregates multi-key bindings per action (so `KeyA`/`ArrowLeft` both driving `MoveLeft` don't cause a spurious release when only one is let go) and publishes edge-triggered messages. Three consumers subscribe to those messages: `player_input` (movement + jump), `toggle_ui_visibility` (F1), and `request_tuning_validation` (F5).

Researching how two real, actively-maintained open-source Bevy games (Fish Folk's Jumpy and Punchy) implement player input surfaced that Punchy uses `leafwing_input_manager` — the de facto standard input-abstraction crate in the Bevy ecosystem — rather than a hand-rolled solution. It solves exactly the problem `InputController` was built to solve (semantic actions instead of raw key checks, multi-binding-per-action merging, press/release edge detection), with a maintained crate instead of custom code, and adds capabilities we don't have today (analog axis values, gamepad support if ever needed).

**Decision:** replace the hand-rolled `InputController` with `leafwing_input_manager` directly (not a look-alike hand-rolled version — the actual crate).

## Compatibility

`leafwing_input_manager = "0.21"` targets Bevy `0.19` (confirmed against the crate's own `Cargo.toml`), matching this project's `bevy = "0.19"`. Confirmed locally compiling against this project's exact dependency set.

**Important correction found during research:** as of `0.21.0`, the crate's `RELEASES.md` says resource-only `ActionState` usage was removed. Verified directly in the vendored source: `update_action_state<A>` only ever queries `(&mut ActionState<A>, &InputMap<A>)` as *components* on an entity — there is no resource-based path. This matches Punchy's own code, where `MenuAction` (a global, not-tied-to-the-player action set) is still read via `Query<&ActionState<MenuAction>>` + `.single()`, i.e. it's a component on some singleton entity, not a `Resource`. This spec follows that same pattern for our two global actions.

## Architecture

**Action sets** (mirroring Punchy's `PlayerAction`/`MenuAction` split): two `Actionlike` enums, replacing the single `InputAction` enum entirely.

```rust
#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect)]
enum PlayerAction {
    #[actionlike(Axis)]
    Move,
    Jump,
}

#[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash, Debug, Reflect)]
enum DebugAction {
    ToggleDebugUi,
    ValidateTuning,
}
```

`Move` is a single signed axis (`-1.0..=1.0`), not two booleans — the project only has left/right movement, so a `#[actionlike(Axis)]` variant matches the domain directly and drops the `Local<MoveDirState>` bookkeeping `player_input` currently needs.

**Bindings:**

```rust
// Player
InputMap::default()
    .with_axis(PlayerAction::Move, VirtualAxis::ad())
    .with_axis(PlayerAction::Move, VirtualAxis::horizontal_arrow_keys())
    .with(PlayerAction::Jump, KeyCode::Space)

// Debug (spawned on its own entity, not the player)
InputMap::default()
    .with(DebugAction::ToggleDebugUi, KeyCode::F1)
    .with(DebugAction::ValidateTuning, KeyCode::F5)
```

`VirtualAxis::ad()` and `VirtualAxis::horizontal_arrow_keys()` are crate-provided constructors (`A`/`D` and `ArrowLeft`/`ArrowRight` respectively). Binding the same `PlayerAction::Move` twice, once per virtual axis, is the crate's documented way to let either key set drive the same action — confirmed in the crate's own `default_controls.rs` example, which binds one action to both a gamepad stick and `VirtualDPad::wasd()` the same way. The crate handles the "either physical binding can drive the action, releasing one while the other is held keeps it active" merging internally — this project no longer needs to implement or test that logic itself.

**Where things live:**

- `src/engine/input.rs`: replaced entirely. Holds `PlayerAction`, `DebugAction`, a `DebugInputMarker` component, and a `spawn_debug_input_map` `Startup` system that spawns the one entity carrying `InputMap<DebugAction>` (marked `DebugInputMarker`, so consumers can query it specifically rather than assuming "the only entity with this ActionState").
- `src/engine/player.rs`: `spawn_player` additionally inserts `InputMap::<PlayerAction>::default()...` (built as shown above) into the player entity's bundle. `ActionState<PlayerAction>` is not inserted manually — `InputMap<PlayerAction>` requires it as a component dependency, and the crate adds it automatically.
- `src/engine/systems.rs`: `player_input` rewritten to query `&ActionState<PlayerAction>` instead of messages; `MoveDirState` and its `Local` are deleted.
- `src/engine/debug_ui.rs`: `toggle_ui_visibility` rewritten to query `Query<&ActionState<DebugAction>, With<DebugInputMarker>>`.
- `src/engine/scripting.rs`: `request_tuning_validation` rewritten the same way, checking `DebugAction::ValidateTuning`.
- `src/engine/mod.rs`: register `InputManagerPlugin::<input::PlayerAction>::default()` and `InputManagerPlugin::<input::DebugAction>::default()`; add `input::spawn_debug_input_map` to the `Startup` chain; **remove** the `input::InputSet` system set and every `.after(input::InputSet)` call — no longer needed, since the crate's own `update_action_state` system runs in `PreUpdate`, strictly before this project's `Update` systems read `ActionState`, with no manual ordering required.

## Data Flow

1. `Startup`: `spawn_player` inserts `InputMap<PlayerAction>` (+ auto-added `ActionState<PlayerAction>`) on the player entity; `spawn_debug_input_map` spawns a `DebugInputMarker` entity with `InputMap<DebugAction>` (+ auto-added `ActionState<DebugAction>`).
2. Every frame, in `PreUpdate` (owned entirely by `InputManagerPlugin`, not this project's code), each `ActionState` is refreshed from the current raw input state and the entity's `InputMap`.
3. In `Update`, `player_input` reads `ActionState<PlayerAction>` on the player entity directly; `toggle_ui_visibility` and `request_tuning_validation` read `ActionState<DebugAction>` on the `DebugInputMarker` entity directly. No messages, no manual state, no manual scheduling order.

## Error Handling

If either singleton entity doesn't exist yet (e.g. a system runs before `Startup` finishes, which shouldn't happen in practice but is defensive), `Query::single()`/`single_mut()` returns `Err`, and each consumer early-returns exactly as it does today when the player entity doesn't exist — no new failure mode introduced.

## Testing

Rewritten using the crate's own tested pattern (verified against its `tests/buttonlike.rs`), not our previous manual `ButtonInput` manipulation:

- Test app setup: `App::new().add_plugins((MinimalPlugins, bevy::input::InputPlugin, InputManagerPlugin::<A>::default()))`, spawn an entity with the relevant `InputMap<A>`.
- Simulate a key press with the crate's own `Buttonlike` extension: `KeyCode::KeyD.press(app.world_mut())` / `.release(app.world_mut())`, then `app.update()`.
- Assert via `ActionState<A>` queried back out (`action_state.clamped_value(&PlayerAction::Move)`, `.just_pressed(&PlayerAction::Jump)`, etc.).

The two existing `input.rs` tests that verify "releasing one of two keys bound to the same action doesn't spuriously fire a release while the other is still held" are **deleted**, not migrated — that behavior is now the crate's responsibility, already covered by the crate's own test suite, and re-testing a dependency's internals isn't this project's job. `player_input`, `toggle_ui_visibility`, and `request_tuning_validation`'s existing tests are rewritten to the new query-based shape, keeping the same behavioral assertions (scripted move speed/jump force are honored; F1 flips `UiVisible`; F5 sends the `get_tuning` callback request).

## Out of Scope

- Gamepad bindings (the crate supports them; this migration only ports the existing keyboard-only behavior).
- Any change to what `PlayerAction`/`DebugAction` actually do downstream (gameplay behavior is unchanged; this is purely how input reaches the same three consumers).
- Networking/rollback integration (irrelevant — MarioEngine is single-player).
