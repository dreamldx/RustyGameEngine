-- Generic loader for `.yscn` scene files: a YAML description of level
-- entities (PlayerSpawn, Platform) that gets converted into engine entities
-- via the same `world.*` bindings imperative level scripts use directly
-- (see level.rs). A level script loads a scene with `yscn.load(path)`, may
-- query/mutate its entities, then calls `scene:spawn_all()`.
local tinyyaml = require("tinyyaml")

local Scene = {}
Scene.__index = Scene

--- Returns the first entity whose `name` field equals `name`, or nil if none
--- match (or the entity has no `name`).
function Scene:find(name)
    for _, entity in ipairs(self.entities) do
        if entity.name == name then
            return entity
        end
    end
    return nil
end

-- tinyyaml's flow-style parser (`{...}`/`[...]`) doesn't apply its own
-- number/bool coercion the way block-style scalars do, so every number in
-- our (flow-style) position/points/color fields comes back as a string.
-- Coerce here, at the yscn->engine boundary, rather than patching that
-- further into the vendored parser.
local function num(v)
    if type(v) == "number" then
        return v
    end
    local n = tonumber(v)
    if not n then
        error("yscn: expected a number, got '" .. tostring(v) .. "'")
    end
    return n
end

local function to_points(points)
    local out = {}
    for i, p in ipairs(points) do
        out[i] = { x = num(p.x), y = num(p.y) }
    end
    return out
end

local function to_color(color)
    local out = {}
    for i, c in ipairs(color) do
        out[i] = num(c)
    end
    return out
end

--- Converts every entity into an engine entity via `world.*`. Call after
--- any `Scene:find(...)` mutations you want reflected in the spawned result.
function Scene:spawn_all()
    for _, entity in ipairs(self.entities) do
        if entity.type == "PlayerSpawn" then
            world.set_player_spawn(num(entity.position.x), num(entity.position.y))
        elseif entity.type == "Platform" then
            world.spawn_platform(
                num(entity.position.x),
                num(entity.position.y),
                to_points(entity.points),
                to_color(entity.color)
            )
        elseif entity.type == "LevelBounds" then
            world.set_level_bounds(num(entity.min_x), num(entity.max_x))
        else
            error("yscn: unknown entity type '" .. tostring(entity.type) .. "'")
        end
    end
end

local M = {}

--- Reads and parses the `.yscn` file at `path` (relative to the project
--- root, e.g. "assets/levels/data/level1.yscn") into a Scene.
function M.load(path)
    local file, err = io.open(path, "r")
    if not file then
        error("yscn: failed to open '" .. path .. "': " .. tostring(err))
    end
    local source = file:read("a")
    file:close()

    local data = tinyyaml.parse(source)
    return setmetatable({ format = data.format, entities = data.entities }, Scene)
end

return M
