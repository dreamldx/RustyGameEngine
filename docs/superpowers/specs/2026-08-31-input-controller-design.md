# Input Controller (Pub/Sub Action Messages) — Design

## Context

MarioEngine currently has three independent places that read `Res<ButtonInput<KeyCode>>` directly:

- `systems::player_input` — `KeyA`/`KeyD`/`ArrowLeft`/`ArrowRight` via `.pressed()` (continuous, held-state) for movement, `Space` via `.just_pressed()` gated on `Grounded` for jump.
- `debug_ui::toggle_ui_visibility` — `F1` via `.just_pressed()`, flips `UiVisible`.
- `scripting::request_tuning_validation` — `F5` via `.just_pressed()`, sends the `get_tuning` callback request.

Each of these hardcodes its own physical key(s) and reads raw input directly. This spec introduces a single `InputController` layer that owns all physical key polling, and refactors all three existing handlers to react to semantic action messages instead of reading `ButtonInput` themselves.

## Goal

Centralize keyboard polling into one publisher, and have every existing input-driven system become a subscriber to semantic action messages, so that:

- Physical key bindings live in exactly one place.
- Handlers depend on *what the player did* (`Jump`, `ToggleDebugUi`, …), not *which key* does it.
- Future input-driven features (enemy AI debug toggles, additional menu shortcuts, etc.) plug into the same pub/sub mechanism rather than adding another direct `ButtonInput` read site.

## Architecture

**New module:** `src/engine/input.rs`.

**`InputAction` enum** (`Copy, Clone, PartialEq, Eq, Hash, Debug`):
`MoveLeft`, `MoveRight`, `Jump`, `ToggleDebugUi`, `ValidateTuning`.

**Messages** (this Bevy version's `Event` → `Message` rename; registered via `app.add_message::<T>()`):
- `ActionPressed(pub InputAction)`
- `ActionReleased(pub InputAction)`

**Key bindings** — a fixed table, `const KEY_BINDINGS: &[(KeyCode, InputAction)]`:

| Key(s) | Action |
|---|---|
| `KeyA`, `ArrowLeft` | `MoveLeft` |
| `KeyD`, `ArrowRight` | `MoveRight` |
| `Space` | `Jump` |
| `F1` | `ToggleDebugUi` |
| `F5` | `ValidateTuning` |

**`publish_input_actions` system (the "pub" side):** runs every `Update`, reads `Res<ButtonInput<KeyCode>>`, and for each `InputAction`, aggregates whether *any* of its bound keys are currently held (`.pressed()`). It compares that aggregate against the previous frame's aggregate (kept in `Local<HashMap<InputAction, bool>>`) and writes `ActionPressed`/`ActionReleased` only on a `false→true`/`true→false` transition of the aggregate — not per physical key.

This aggregation is required because `MoveLeft` has two bound keys. Without it, releasing `KeyA` while `ArrowLeft` is still held would incorrectly emit `ActionReleased(MoveLeft)`, since a naive per-key mapping has no notion of "the action is still logically held by a different key."

## Components (subscriber refactor)

All three existing handlers stop reading `ButtonInput<KeyCode>` and instead read `MessageReader<ActionPressed>` (and `ActionReleased` where relevant):

- **`player_input`** (`systems.rs`): adds a `Local<MoveDirState>` (`left: bool, right: bool, jump_requested: bool`, all default `false`). On each `ActionPressed`/`ActionReleased` for `MoveLeft`/`MoveRight`, updates `left`/`right`. On `ActionPressed(Jump)`, sets `jump_requested = true`. Movement direction and jump are then computed from `MoveDirState` exactly as they previously were from raw key state; `jump_requested` is consumed (reset to `false`) once processed, whether or not the player was grounded (matches the current behavior of `just_pressed`, which only fires once per press regardless of ground state).
- **`toggle_ui_visibility`** (`debug_ui.rs`): reads `ActionPressed`, flips `UiVisible` when it sees `ToggleDebugUi`.
- **`request_tuning_validation`** (`scripting.rs`): reads `ActionPressed`, proceeds with the existing request logic when it sees `ValidateTuning`.

**Scheduling:** `publish_input_actions` is registered with explicit `.before(...)` on all three consumer systems, so the message is available to readers in the same frame it's published (no one-frame input lag).

## Data Flow

1. Every `Update`, `publish_input_actions` reads `ButtonInput<KeyCode>`, recomputes the held-aggregate per `InputAction`, and diffs against last frame's aggregate (`Local` state).
2. Each transition writes an `ActionPressed` or `ActionReleased` message.
3. Later in the same frame (enforced by `.before(...)`), `player_input`, `toggle_ui_visibility`, and `request_tuning_validation` each read the messages relevant to them and update their own state/behavior.

## Error Handling

None of this introduces fallible operations beyond what already exists — `MessageReader`/`MessageWriter` don't fail. An `InputAction` with no bound key (a future addition someone forgets to wire up) simply never fires; this is a silent no-op rather than a panic, consistent with how the current direct-`ButtonInput` code already silently does nothing for unbound keys.

## Testing

This is pure ECS logic with no BMS/egui dependency, so it's fully unit-testable with a Bevy `App` test harness (the same pattern used for `ScriptedTuning` in an earlier sub-project):

1. `publish_input_actions` aggregation: press `KeyA`, run, assert `ActionPressed(MoveLeft)`; press `ArrowLeft` too (both held), run, assert no new message (aggregate unchanged); release `KeyA` (still holding `ArrowLeft`), run, assert **no** `ActionReleased(MoveLeft)`; release `ArrowLeft`, run, assert `ActionReleased(MoveLeft)` now fires. This is the dedup case the design exists to handle.
2. `player_input`: inject `ActionPressed(MoveLeft)`/`ActionPressed(Jump)` via `MessageWriter` directly (bypassing real key state) and assert velocity updates exactly as the existing Task 2 tests already check — those three existing tests get rewritten to publish messages instead of pressing `ButtonInput` keys directly.
3. `toggle_ui_visibility` / `request_tuning_validation`: same substitution — inject the relevant `ActionPressed` message and assert the existing behavior (flip `UiVisible`; send the validation request) still occurs, replacing their current `ButtonInput`-based manual test setup if any exists, or added fresh if not.

## Out of Scope

- Configurable/remappable bindings (e.g., loading a keybinding file). The binding table is a fixed Rust const for now.
- Gamepad or mouse input.
- Any change to the two egui menu-bar buttons (Reload Level / Exit Game) — those are UI clicks, not keyboard actions, and stay as direct `MessageWriter`/resource-flag calls from the egui click handlers.
