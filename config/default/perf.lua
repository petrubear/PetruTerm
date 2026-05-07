-- PetruTerm performance configuration

local module = {}

function module.apply_to_config(config)
  -- Number of lines kept in terminal scrollback history.
  -- Each terminal uses ~200 B/line; 5 000 lines ≈ 1 MB per pane.
  -- With 20 panes open that is ~20 MB. Raise carefully: 50 000 = ~200 MB total.
  config.scrollback_lines  = 5000

  -- Show a scroll position indicator on the right edge of the terminal.
  config.enable_scroll_bar = true

  -- Maximum frames per second for the GPU render loop.
  config.max_fps           = 60

  -- Animation frame rate (set low for snappiness; increase for smooth transitions).
  config.animation_fps     = 1

  -- GPU power preference: "high_performance" | "low_power" | "none"
  -- Selects the wgpu GPU adapter at startup. Use "low_power" (default) to
  -- prefer the integrated / efficiency GPU for best battery life.
  -- Use "high_performance" if you need the discrete GPU (e.g. eGPU or dual-GPU Mac).
  -- Note: changing this requires a restart to take effect.
  config.gpu_preference    = "low_power"

  -- Shell to launch in new tabs. Defaults to $SHELL or /bin/zsh.
  config.shell             = os.getenv("SHELL") or "/bin/zsh"

  -- Inject shell integration (CWD tracking, exit codes, last command).
  config.shell_integration = true


  -- Show dirty indicator (*) next to the git branch name in the status bar.
  -- Requires running `git status --porcelain` every 5 s — costs an extra subprocess.
  -- Enable if you want the indicator; leave false to save CPU/battery.
  config.status_bar.git_dirty_check = false

  -- Battery saver mode: "auto" | "always" | "never"
  -- "auto": when on battery, disables git_dirty_check, extends git poll TTL to 60 s,
  --         slows cursor blink to 750 ms, switches present mode to Fifo (vsync),
  --         and shows a BAT XX% indicator in the status bar.
  -- "always": apply restrictions regardless of power source.
  -- "never":  never apply restrictions.
  config.battery_saver = "auto"
end

return module
