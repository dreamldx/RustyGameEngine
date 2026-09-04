move_speed = 300.0
jump_force = 800.0
gravity = 2080.0

function on_script_loaded()
    world.set_player_tuning(move_speed, jump_force, gravity)
end

function on_script_reloaded()
    world.set_player_tuning(move_speed, jump_force, gravity)
end
