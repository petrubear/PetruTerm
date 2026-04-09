-- PetruTerm keybinds configuration
-- petruterm-config-version: 3
-- Leader key: Ctrl+F, 1000ms timeout.
-- After pressing Ctrl+F, press the bound key within the timeout window.
--
-- System keybinds that remain hardcoded (not configurable here):
--   Cmd+C / Cmd+V   — copy / paste (clipboard)
--   Cmd+Q           — quit
--   Cmd+1-9         — switch to tab N

local petruterm = require("petruterm")
local module    = {}

function module.apply_to_config(config)
  config.leader = { key = "f", mods = "CTRL", timeout_ms = 1000 }

  config.keys = {
    -- ── Overlays ──────────────────────────────────────────────────────────
    { mods = "LEADER", key = "o",  action = petruterm.action.CommandPalette },

    -- ── AI panel ──────────────────────────────────────────────────────────
    { mods = "LEADER", key = "a",  action = petruterm.action.ToggleAiPanel },  -- abrir / cerrar
    { mods = "LEADER", key = "A",  action = petruterm.action.FocusAiPanel  },  -- mover focus terminal ↔ chat

    -- ── AI context actions ─────────────────────────────────────────────────
    { mods = "LEADER", key = "e",  action = petruterm.action.ExplainLastOutput },
    { mods = "LEADER", key = "f",  action = petruterm.action.FixLastError },
    { mods = "LEADER", key = "z",  action = petruterm.action.UndoLastWrite },

    -- ── Tabs (tmux-style) ─────────────────────────────────────────────────
    { mods = "LEADER", key = "c",  action = petruterm.action.NewTab },
    { mods = "LEADER", key = "&",  action = petruterm.action.CloseTab },
    { mods = "LEADER", key = "n",  action = petruterm.action.NextTab },
    { mods = "LEADER", key = "b",  action = petruterm.action.PrevTab },

    -- ── Pane splits (tmux-style) ───────────────────────────────────────────
    { mods = "LEADER", key = "%",  action = petruterm.action.SplitHorizontal },
    { mods = "LEADER", key = '"',  action = petruterm.action.SplitVertical },
    { mods = "LEADER", key = "x",  action = petruterm.action.ClosePane },

    -- ── Pane focus (vim-style) ─────────────────────────────────────────────
    { mods = "LEADER", key = "h",  action = petruterm.action.FocusPaneLeft },
    { mods = "LEADER", key = "j",  action = petruterm.action.FocusPaneDown },
    { mods = "LEADER", key = "k",  action = petruterm.action.FocusPaneUp },
    { mods = "LEADER", key = "l",  action = petruterm.action.FocusPaneRight },
  }
end

return module
