-- Dedicated input script: pushes the player's input bindings to the engine.
-- Bindings live in config/player_input.lua; re-save THIS file to hot-reload them.
-- dofile (not require) so a hot reload re-reads the config fresh instead of
-- hitting require's package.loaded cache.
local input_config = dofile("assets/scripts/config/player_input.lua")

-- Fired explicitly by the engine once the player entity exists (see
-- player::broadcast_scene_loaded) — unlike BMS's automatic on_script_loaded,
-- which fires as soon as this script loads, before the player is spawned.
-- Covers initial startup and every "Reload Level".
function on_scene_loaded()
    world.set_player_input(input_config)
end

-- BMS's automatic hot-reload hook, fired when this file is re-saved while
-- the app is running. By then the player already exists (hot-editing only
-- happens after the game has been running for a while), so it's safe to
-- push directly here too — this is what gives player_input.lua live edits.
function on_script_reloaded()
    world.set_player_input(input_config)
end
