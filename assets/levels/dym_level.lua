-- Procedurally generates a platformer level as a directed graph of jumps:
-- each platform can jump *to* at most MAX_OUT_DEGREE others, and be jumped
-- *to from* at most MAX_IN_DEGREE others. Building it as a graph (rather
-- than picking N independent random positions) is what guarantees every
-- platform is actually reachable — each new platform's position is a 2D
-- offset from an existing platform that still has jump capacity, so by
-- construction it always lands within a plausible jump of something
-- already placed. Re-generates (with a new random layout) every time this
-- level (re)loads.

local MAX_PLATFORMS = 30
local MAX_OUT_DEGREE = 2
local MAX_IN_DEGREE = 3

-- Matches player.lua's jump physics (jump_force=800, gravity=2080,
-- move_speed=300). Coordinates are y-up (see on_level_load(), where a platform's
-- spawn point is placed *above* it by adding half_h), so a positive dy is
-- a rise and a negative dy is a fall.
local JUMP_FORCE = 800.0
local GRAVITY = 2080.0
local MOVE_SPEED = 300.0

-- Max achievable rise: v0^2 / (2*g) ≈ 153.8. We keep a small safety margin
-- below the true physical ceiling so near-limit samples are still
-- comfortably makeable rather than pixel-perfect.
local MAX_JUMP_RISE = 140.0
-- Falling isn't limited by jump force at all (gravity keeps accelerating
-- the player down for as long as they're airborne) — this cap exists only
-- to keep generated drops from becoming absurdly long, not because a
-- bigger fall would be physically infeasible.
local MAX_JUMP_FALL = 500.0

-- Gaussian mean/stddev for sampling a candidate vertical offset before it's
-- checked for physical feasibility. Mean 0 with a wide spread means new
-- platforms are placed above *or* below their parent with roughly equal
-- likelihood, instead of clustering at one height band — this is what
-- produces multiple stacked "layers" rather than one flat corridor.
local JUMP_DY_MEAN, JUMP_DY_STDDEV = 0.0, 160.0
-- Horizontal offset is still sampled independently around the flat-jump
-- distance, but every candidate is then hard-capped by
-- max_horizontal_for_dy() below, so a sample that's fine at dy=0 will get
-- rejected if paired with a dy near MAX_JUMP_RISE.
local JUMP_DX_MEAN, JUMP_DX_STDDEV = 220.0, 90.0

local PLATFORM_MIN_HALF_W, PLATFORM_MAX_HALF_W = 140.0, 260.0
-- Kept thin on purpose (height 12–20 total, down from the original
-- 24–40) so platforms read as ledges rather than blocks.
local PLATFORM_MIN_HALF_H, PLATFORM_MAX_HALF_H = 6.0, 10.0

-- Chance, per new platform, of also linking a second nearby existing
-- platform as an extra jump source into it (on top of the one it was
-- generated from) — this is what lets some platforms end up reachable
-- from more than one place, up to MAX_IN_DEGREE, instead of the graph
-- always being a strict one-parent-per-platform tree.
local EXTRA_LINK_PROBABILITY = 0.25

-- Minimum gap enforced between two platforms, but only when they're close
-- enough vertically that a player could plausibly mistake them for being
-- on the same "floor" — platforms on genuinely different height layers
-- don't need horizontal separation from each other, since a player at one
-- height never perceives or reaches for a platform far above/below it.
-- (An earlier version of this check ignored vertical distance entirely,
-- requiring a fixed horizontal gap between literally every pair of
-- platforms in the level. That's a much stronger constraint than it
-- sounds: on a fixed-width level it caps the *total* platform count at
-- roughly level_width / (gap + 2*avg_half_width), because it forces every platform to claim an
-- exclusive horizontal slice of the level regardless of height, making
-- dense vertical stacking (the whole point of "more layers") impossible.
-- Tying it back to vertical proximity restores that.)
local PLATFORM_MARGIN_X = 120.0
local PLATFORM_MARGIN_Y = 80.0

-- The whole level is generated inside a square region, so its overall
-- footprint is always width == height regardless of how the random walk
-- happens to grow. The square's bottom-left corner is pinned to the spawn
-- platform's own bottom-left edge (set up in on_level_load()), so the player always
-- starts in the level's bottom-left corner and the level only grows up and
-- to the right from there. Any candidate that would poke outside this
-- square is rejected just like an overlap or an infeasible jump.
local LEVEL_SIZE = 5000.0

-- How many different (parent, offset) candidates to try before giving up on
-- placing one more platform and ending generation early.
local MAX_PLACEMENT_ATTEMPTS = 30

local PLAYER_HALF_HEIGHT = 24.0 -- see player.rs PLAYER_SIZE

--- Standard Box-Muller transform: samples from a normal distribution with
--- the given mean/stddev. Lua's stdlib has no built-in Gaussian sampler.
local function gaussian(mean, stddev)
    local u1 = math.max(math.random(), 1e-12)
    local u2 = math.random()
    local z0 = math.sqrt(-2.0 * math.log(u1)) * math.cos(2.0 * math.pi * u2)
    return mean + z0 * stddev
end

--- Given a vertical displacement `dy` (positive = up, negative = down),
--- returns the max horizontal distance a jump can cover while still being
--- at that height at some point during the arc, or nil if `dy` is
--- physically unreachable (i.e. above MAX_JUMP_RISE — a taller rise than
--- the jump force allows).
---
--- Height is a function of time alone (y(t) = v0*t - 0.5*g*t^2), independent
--- of however the player moves horizontally, so at any instant t the
--- reachable x range is [-move_speed*t, move_speed*t]. Solving y(t) = dy
--- gives two crossing times — an earlier one on the way up and a later one
--- on the way down (or just one, past the arc's peak, if dy < 0). We take
--- the LATER (larger) root: whichever time the platform is actually hit
--- at, the reachable-x window at that time is the widest one, and it's a
--- superset of the earlier window, so it's the correct feasibility bound.
local function max_horizontal_for_dy(dy)
    if dy > MAX_JUMP_RISE then
        return nil
    end
    local discriminant = JUMP_FORCE * JUMP_FORCE - 2.0 * GRAVITY * dy
    if discriminant < 0 then
        return nil
    end
    local sqrt_d = math.sqrt(discriminant)
    local t = (JUMP_FORCE + sqrt_d) / GRAVITY
    return MOVE_SPEED * t
end

--- Feasibility check used for both the primary parent link and any extra
--- links: rejects outright rather than clamping, so accepted samples stay
--- genuinely Gaussian-shaped instead of piling up at a boundary.
local function jump_is_feasible(dx, dy)
    if dy < -MAX_JUMP_FALL then
        return false
    end
    local max_dx = max_horizontal_for_dy(dy)
    if max_dx == nil then
        return false
    end
    return math.abs(dx) <= max_dx
end

local function random_color()
    return {
        0.3 + math.random() * 0.6,
        0.3 + math.random() * 0.6,
        0.3 + math.random() * 0.6,
        1.0,
    }
end

--- Removes `frontier[slot]` without preserving order (swap with the last
--- element and pop) — cheap, and selection from `frontier` is already
--- uniform-random so order never mattered.
local function frontier_remove_at(frontier, slot)
    frontier[slot] = frontier[#frontier]
    frontier[#frontier] = nil
end

--- True if `a` and `b` are too close: within PLATFORM_MARGIN_X of each
--- other horizontally AND within PLATFORM_MARGIN_Y of each other
--- vertically. Both conditions must hold — this is a standard AABB
--- overlap-with-margin test, not a distance-in-isolation check — so the
--- 60-unit horizontal rule only bites for platforms near the same height;
--- see PLATFORM_MARGIN_X's comment for why that matters.
local function platforms_too_close(a, b)
    return math.abs(a.x - b.x) < (a.half_w + b.half_w + PLATFORM_MARGIN_X)
        and math.abs(a.y - b.y) < (a.half_h + b.half_h + PLATFORM_MARGIN_Y)
end

--- True if `candidate` (with its own half_w/half_h) fits entirely inside
--- the square level bounds [min_x, max_x] x [min_y, max_y].
local function within_level_bounds(candidate, min_x, min_y, max_x, max_y)
    return candidate.x - candidate.half_w >= min_x
        and candidate.x + candidate.half_w <= max_x
        and candidate.y - candidate.half_h >= min_y
        and candidate.y + candidate.half_h <= max_y
end

local function too_close_to_any(candidate, platforms)
    for _, p in ipairs(platforms) do
        if platforms_too_close(candidate, p) then
            return true
        end
    end
    return false
end

function on_level_load()
    math.randomseed(os.time() + math.floor(os.clock() * 1000000))

    local platforms = {}
    local frontier = {} -- indices into `platforms` with out_degree < MAX_OUT_DEGREE

    -- Platform 1 is where the player spawns, directly above its center —
    -- satisfies "the initial player must land on a platform" by construction
    -- rather than by chance.
    local start_half_w, start_half_h = 150.0, 20.0
    platforms[1] = {
        x = 200.0,
        y = 0.0,
        half_w = start_half_w,
        half_h = start_half_h,
        color = { 0.55, 0.35, 0.15, 1.0 },
        out_degree = 0,
        in_degree = 0,
    }
    table.insert(frontier, 1)
    world.set_player_spawn(
        platforms[1].x,
        platforms[1].y + start_half_h + PLAYER_HALF_HEIGHT + 4.0
    )

    -- Square bounds are pinned so the spawn platform's own bottom-left
    -- edge IS the level's bottom-left corner — nothing is ever generated
    -- below or to the left of it, so the player always starts in the
    -- level's bottom-left corner and everything grows up/right from there.
    local level_min_x = platforms[1].x - start_half_w
    local level_min_y = platforms[1].y - start_half_h
    local level_max_x = level_min_x + LEVEL_SIZE
    local level_max_y = level_min_y + LEVEL_SIZE

    -- A single round of MAX_PLACEMENT_ATTEMPTS failing is an expected,
    -- fairly likely event on its own (the distance+overlap filters together
    -- reject the large majority of raw candidates) — it must not end
    -- generation by itself, or the level would almost always stop after only
    -- a handful of platforms purely from bad luck. Only genuine saturation
    -- (many consecutive rounds all failing) should stop it early.
    local consecutive_failures = 0
    local MAX_CONSECUTIVE_FAILURES = 50

    while #platforms < MAX_PLATFORMS and #frontier > 0 do
        -- Try several (parent, offset, size) candidates until one doesn't
        -- overlap an existing platform and is within jump range.
        local frontier_slot, parent_index, parent, candidate
        local placed = false
        for _ = 1, MAX_PLACEMENT_ATTEMPTS do
            frontier_slot = math.random(#frontier)
            parent_index = frontier[frontier_slot]
            parent = platforms[parent_index]

            local dx = gaussian(JUMP_DX_MEAN, JUMP_DX_STDDEV)
            if math.random() < 0.5 then
                dx = -dx -- jumps land to either side of the parent, not just one
            end
            local dy = gaussian(JUMP_DY_MEAN, JUMP_DY_STDDEV)
            if jump_is_feasible(dx, dy) then
                candidate = {
                    x = parent.x + dx,
                    y = parent.y + dy,
                    half_w = PLATFORM_MIN_HALF_W + math.random() * (PLATFORM_MAX_HALF_W - PLATFORM_MIN_HALF_W),
                    half_h = PLATFORM_MIN_HALF_H + math.random() * (PLATFORM_MAX_HALF_H - PLATFORM_MIN_HALF_H),
                }
                if within_level_bounds(candidate, level_min_x, level_min_y, level_max_x, level_max_y)
                    and not too_close_to_any(candidate, platforms)
                then
                    placed = true
                    break
                end
            end
        end
        if not placed then
            consecutive_failures = consecutive_failures + 1
            if consecutive_failures >= MAX_CONSECUTIVE_FAILURES then
                break
            end
            goto continue
        end
        consecutive_failures = 0

        candidate.color = random_color()
        candidate.out_degree = 0
        candidate.in_degree = 1
        candidate.parents = { parent_index }
        local new_index = #platforms + 1
        platforms[new_index] = candidate
        local new_x, new_y = candidate.x, candidate.y

        parent.out_degree = parent.out_degree + 1
        if parent.out_degree >= MAX_OUT_DEGREE then
            frontier_remove_at(frontier, frontier_slot)
        end
        table.insert(frontier, new_index)

        -- Keep adding extra jump sources into this platform — each
        -- attempt independently rolls EXTRA_LINK_PROBABILITY — so in_degree
        -- can actually reach MAX_IN_DEGREE (not just parent + a single
        -- extra), same as out_degree can actually reach MAX_OUT_DEGREE.
        while platforms[new_index].in_degree < MAX_IN_DEGREE and math.random() < EXTRA_LINK_PROBABILITY do
            local candidates = {}
            for i, p in ipairs(platforms) do
                local already_linked = false
                for _, linked in ipairs(candidate.parents) do
                    if linked == i then
                        already_linked = true
                        break
                    end
                end
                if i ~= new_index and not already_linked and p.out_degree < MAX_OUT_DEGREE then
                    -- Extra link must itself be a physically feasible jump
                    -- *from* the existing platform *to* the new one.
                    if jump_is_feasible(new_x - p.x, new_y - p.y) then
                        table.insert(candidates, i)
                    end
                end
            end
            if #candidates == 0 then
                break
            end

            local extra_parent_index = candidates[math.random(#candidates)]
            local extra_parent = platforms[extra_parent_index]
            extra_parent.out_degree = extra_parent.out_degree + 1
            candidate.in_degree = candidate.in_degree + 1
            table.insert(candidate.parents, extra_parent_index)
            if extra_parent.out_degree >= MAX_OUT_DEGREE then
                for slot, idx in ipairs(frontier) do
                    if idx == extra_parent_index then
                        frontier_remove_at(frontier, slot)
                        break
                    end
                end
            end
        end

        ::continue::
    end

    for _, p in ipairs(platforms) do
        world.spawn_platform(
            p.x,
            p.y,
            {
                { x = -p.half_w, y = -p.half_h },
                { x = p.half_w, y = -p.half_h },
                { x = p.half_w, y = p.half_h },
                { x = -p.half_w, y = p.half_h },
            },
            p.color
        )
    end
end