-- PetruTerm keybinds configuration
-- petruterm-config-version: 2
-- Leader key: Ctrl+B (tmux-style), 1000ms timeout.
-- After pressing Ctrl+B, press the bound key within the timeout window.
--
-- System keybinds that remain hardcoded (not configurable here):
--   Cmd+C / Cmd+V   — copy / paste (clipboard)
--   Cmd+Q           — quit
--   Cmd+1-9         — switch to tab N

local petruterm = require("petruterm")
local module    = {}

function module.apply_to_config(config)
  config.leader = { key = "b", mods = "CTRL", timeout_ms = 1000 }

  config.keys = {
    -- ── Overlays ──────────────────────────────────────────────────────────
    { mods = "LEADER", key = "p",  action = petruterm.action.CommandPalette },

    -- ── AI panel (open → focus → close cycle) ─────────────────────────────
    { mods = "LEADER", key = "a",  action = petruterm.action.ToggleAiPanel },

    -- ── AI context actions ─────────────────────────────────────────────────
    { mods = "LEADER", key = "e",  action = petruterm.action.ExplainLastOutput },
    { mods = "LEADER", key = "f",  action = petruterm.action.FixLastError },

    -- ── Tabs ───────────────────────────────────────────────────────────────
    { mods = "LEADER", key = "t",  action = petruterm.action.NewTab },
    { mods = "LEADER", key = "w",  action = petruterm.action.CloseTab },
    { mods = "LEADER", key = "n",  action = petruterm.action.NextTab },
    { mods = "LEADER", key = "b",  action = petruterm.action.PrevTab },

    -- ── Pane splits (tmux-style) ───────────────────────────────────────────
    { mods = "LEADER", key = "%",  action = petruterm.action.SplitHorizontal },
    { mods = "LEADER", key = '"',  action = petruterm.action.SplitVertical },
    { mods = "LEADER", key = "x",  action = petruterm.action.ClosePane },
  }
end

return module
