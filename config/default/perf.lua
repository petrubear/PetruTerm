-- PetruTerm performance configuration

local module = {}

function module.apply_to_config(config)
  -- Number of lines kept in terminal scrollback history.
  -- Each terminal uses ~200 B/line; 10 000 lines ≈ 2 MB per pane.
  -- With 20 panes open that is ~40 MB. Raise carefully: 50 000 = ~200 MB total.
  config.scrollback_lines  = 10000

  -- Show a scroll position indicator on the right edge of the terminal.
  config.enable_scroll_bar = true

  -- Maximum frames per second for the GPU render loop.
  -- PetruTerm will not render more than this many frames per second regardless
  -- of how fast PTY data or input events arrive. Lower values save battery.
  config.max_fps           = 60

  -- GPU power preference: "high_performance" | "low_power" | "none"
  -- Selects the wgpu GPU adapter at startup (wgpu binary only). Use "low_power"
  -- (default) to prefer the integrated / efficiency GPU for best battery life.
  -- Use "high_performance" if you need the discrete GPU (e.g. eGPU or dual-GPU Mac).
  -- Note: changing this requires a restart to take effect.
  config.gpu_preference    = "low_power"

  -- Show dirty indicator (*) next to the git branch name in the status bar.
  -- Requires running `git status --porcelain` every 5 s — costs an extra subprocess.
  -- Set to false to save CPU/battery.
  config.status_bar.git_dirty_check = true

  -- Battery saver mode: "auto" | "always" | "never"
  -- "auto": when on battery, disables git_dirty_check, extends git poll TTL to 60 s,
  --         slows cursor blink, switches present mode to Fifo (vsync, wgpu binary),
  --         and shows a BAT XX% indicator in the status bar.
  -- "always": apply restrictions regardless of power source.
  -- "never":  never apply restrictions.
  config.battery_saver = "auto"
end

return module
