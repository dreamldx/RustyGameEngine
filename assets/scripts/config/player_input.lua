-- Player input bindings, imported by player_input.lua via dofile (not loaded as a
-- BMS script entity — load_scripts only scans the top level of scripts/).
-- Key names are Bevy KeyCode variant names, e.g. "KeyA", "Digit3",
-- "ArrowLeft", "Space", "ShiftLeft", "F5".
-- NOTE: edits here hot-reload only after player_input.lua is re-saved (the asset
-- watcher only tracks player_input.lua, which re-imports this file on reload).
return {
    move = {
        axes = {
            { neg = "KeyA", pos = "KeyD" },
            { neg = "ArrowLeft", pos = "ArrowRight" },
        },
    },
    jump = { "Space" },
}
