-- Loads its geometry from a `.yscn` scene file via the generic yscn loader,
-- instead of describing platforms imperatively like `dym_level.lua` does.
local yscn = require("loader.yscn")

-- Matches the window's configured width (see WindowResolution in main.rs) —
-- this level is meant to fit in a single screen, no horizontal scrolling.
local SCREEN_WIDTH = 1920.0

function on_level_load()
    world.set_level_bounds(0.0, SCREEN_WIDTH)

    local scene = yscn.load("assets/levels/data/level1.yscn")

    -- Demonstrates querying/mutating a yscn entity before it's spawned.
    local ground = scene:find("Ground")
    if ground then
        ground.color = { 0.9, 0.1, 0.1, 1.0 }
    end

    scene:spawn_all()
end
