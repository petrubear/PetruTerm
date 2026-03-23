-- PetruTerm keybinds configuration
-- Leader key: Ctrl+B (tmux-style), 1000ms timeout

local petruterm = require("petruterm")
local module    = {}

function module.apply_to_config(config)
  config.leader = { key = "b", mods = "CTRL", timeout_ms = 1000 }

  config.keys = {
    -- Command palette
    { mods = "CMD|SHIFT", key = "P",     action = petruterm.action.CommandPalette },

    -- Pane splits (tmux-style, via leader)
    { mods = "LEADER",    key = "%",     action = petruterm.action.SplitHorizontal },
    { mods = "LEADER",    key = '"',     action = petruterm.action.SplitVertical },

    -- Pane navigation (vim-style, via leader)
    { mods = "LEADER",    key = "h",     action = petruterm.action.ActivatePane },
    { mods = "LEADER",    key = "l",     action = petruterm.action.ActivatePane },
    { mods = "LEADER",    key = "k",     action = petruterm.action.ActivatePane },
    { mods = "LEADER",    key = "j",     action = petruterm.action.ActivatePane },
    { mods = "LEADER",    key = "x",     action = petruterm.action.ClosePane },

    -- Tabs
    { mods = "CMD",       key = "t",     action = petruterm.action.NewTab },
    { mods = "CMD",       key = "w",     action = petruterm.action.CloseTab },

    -- Window
    { mods = "CMD",       key = "q",     action = "Quit" },
  }
end

return module
