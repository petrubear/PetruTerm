-- PetruTerm UI configuration
-- Controls font, colors, window appearance.

local petruterm = require("petruterm")
local module    = {}

function module.apply_to_config(config)
  -- Font
  config.font      = petruterm.font("JetBrainsMono Nerd Font Mono")   -- Override with "Monolisa Nerd Font" if installed
  config.font_size = 15

  -- HarfBuzz OpenType features: contextual alternates, ligatures, discretionary ligatures
  config.font_features = { "calt=1", "liga=1", "dlig=1" }

  -- Color scheme (Dracula Pro)
  config.colors = {
    foreground    = "#f8f8f2",
    background    = "#22212c",
    cursor_bg     = "#9580ff",
    cursor_border = "#9580ff",
    cursor_fg     = "#f8f8f2",
    selection_bg  = "#454158",
    selection_fg  = "#c6c6c2",
    ansi    = { "#22212c", "#ff9580", "#8aff80", "#ffff80", "#9580ff", "#ff80bf", "#80ffea", "#f8f8f2" },
    brights = { "#504c67", "#ffaa99", "#a2ff99", "#ffff99", "#aa99ff", "#ff99cc", "#99ffee", "#ffffff" },
  }

  -- Window
  config.window = {
    borderless      = false,
    start_maximized = true,
    title_bar_style = "custom",   -- "custom" | "native" | "none"
    padding         = { left = 20, right = 20, top = 30, bottom = 10 },
    opacity         = 1.0,
  }

  config.enable_tab_bar      = true
  config.hide_tab_bar_if_one = true
end

return module
