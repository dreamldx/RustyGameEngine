function on_script_loaded()
    local transform_type = world.get_type_by_name("Transform")
    local results = world.query():component(transform_type):build()
    for i, result in pairs(results) do
        local entity = result:entity()
        local components = result:components()
        local transform = components[1]

         print(string.format(
                  "entity %d at (%.1f, %.1f)",
                  entity:index_u32(),
                  transform.translation.x,
                  transform.translation.y
              ))
    end
end