move_speed = 300.0
jump_force = 800.0
gravity = 2080.0

-- Fired explicitly by the engine once the player entity exists (see
-- player::broadcast_scene_loaded) — unlike BMS's automatic on_script_loaded,
-- which fires as soon as this script loads, before the player is spawned.
-- Covers initial startup and every "Reload Level".
function on_scene_loaded()
    world.set_player_tuning(move_speed, jump_force, gravity)
end

-- BMS's automatic hot-reload hook, fired when this file is re-saved while
-- the app is running. By then the player already exists (hot-editing only
-- happens after the game has been running for a while), so it's safe to
-- push directly here too — this is what gives player.lua live tuning edits.
function on_script_reloaded()
    world.set_player_tuning(move_speed, jump_force, gravity)
end
