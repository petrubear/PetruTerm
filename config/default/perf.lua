-- PetruTerm performance configuration

local module = {}

function module.apply_to_config(config)
  config.scrollback_lines  = 10000
  config.enable_scroll_bar = true
  config.max_fps           = 60
  config.animation_fps     = 1           -- Effectively disabled for snappiness
  config.gpu_preference    = "high_performance"
  config.shell_integration = true
end

return module
