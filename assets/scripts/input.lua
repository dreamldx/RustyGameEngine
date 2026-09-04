-- Dedicated input script: pushes the player's input bindings to the engine.
-- Bindings live in config/player_input.lua; re-save THIS file to hot-reload them.
-- dofile (not require) so a hot reload re-reads the config fresh instead of
-- hitting require's package.loaded cache.
local input_config = dofile("assets/scripts/config/player_input.lua")

function on_script_loaded()
    world.set_player_input(input_config)
end

function on_script_reloaded()
    world.set_player_input(input_config)
end
