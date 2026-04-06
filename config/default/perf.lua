-- PetruTerm performance configuration

local module = {}

function module.apply_to_config(config)
  -- Number of lines kept in terminal scrollback history.
  config.scrollback_lines  = 10000

  -- Show a scroll position indicator on the right edge of the terminal.
  config.enable_scroll_bar = true

  -- Maximum frames per second for the GPU render loop.
  config.max_fps           = 60

  -- Animation frame rate (set low for snappiness; increase for smooth transitions).
  config.animation_fps     = 1

  -- GPU power preference: "high_performance" | "low_power" | "none"
  config.gpu_preference    = "high_performance"

  -- Shell to launch in new tabs. Defaults to $SHELL or /bin/zsh.
  config.shell             = os.getenv("SHELL") or "/bin/zsh"

  -- Inject shell integration (CWD tracking, exit codes, last command).
  config.shell_integration = true
end

return module
