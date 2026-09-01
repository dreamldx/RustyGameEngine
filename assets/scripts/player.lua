move_speed = 200.0
jump_force = 500.0
gravity = 980.0

function on_script_loaded()
    world.set_player_tuning(move_speed, jump_force, gravity)
end

function get_tuning()
    return { move_speed = move_speed, jump_force = jump_force, gravity = gravity }
end
