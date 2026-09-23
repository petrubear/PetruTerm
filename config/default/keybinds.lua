-- PetruTerm keybinds configuration
-- petruterm-config-version: 4
-- keybind_style = "tmux" (default): Leader key Ctrl+F, 1000ms timeout.
--   After pressing Ctrl+F, press the bound key within the timeout window.
-- keybind_style = "normal": direct macOS Cmd-combos, no leader key at all
--   (Ctrl+F passes through to the shell instead of activating a leader).
--
-- WARNING: keybind_style must be set HERE, in this file -- setting it in config.lua or
-- elsewhere has no effect (this file's apply_to_config always sets it) and silently disables
-- ALL keybinds (leader and direct). This file is version-managed: PetruTerm overwrites your
-- customized copy whenever the bundled default's version bumps, resetting this back to "tmux".
--
-- System keybinds that remain hardcoded regardless of style (not configurable here):
--   Cmd+C / Cmd+V   — copy / paste (clipboard)
--   Cmd+Q           — quit
--   Cmd+K           — clear screen and scrollback
--   Cmd+F           — open / close text search (Enter=next, Shift+Enter=prev, Esc=close)
--   Cmd+1-9         — switch to tab N
--
-- "normal" style keybinds that are ALSO hardcoded, not config.keys entries below:
-- arrow-key and Return-key presses arrive as named keys, not characters, and
-- KeyBind.key only ever matches a character (same reason the resize gesture
-- below is hardcoded even in tmux style).
--   Cmd+B             — toggle sidebar
--   Cmd+Option+Arrows — focus pane left/right/up/down
--   Cmd+Ctrl+Arrows   — resize focused pane

local petruterm = require("petruterm")
local module    = {}

function module.apply_to_config(config)
  config.keybind_style = "tmux"  -- "tmux" | "normal"
  config.leader = { key = "f", mods = "CTRL", timeout_ms = 1000 }

  -- Keyboard options.
  -- option_as_meta = false (default): Option/Alt acts as a compose key.
  --   Characters like {, }, @, # produced via Option on non-US keyboards work correctly.
  -- option_as_meta = true: Option/Alt sends ESC prefix (Meta key for Emacs/readline).
  config.keyboard = { option_as_meta = false }

  if config.keybind_style == "tmux" then
    config.keys = {
      -- ── Overlays ──────────────────────────────────────────────────────────
      { mods = "LEADER", key = "o",  action = petruterm.action.CommandPalette },

      -- ── AI controls ────────────────────────────────────────────────────────
      -- leader+A   : focus AI panel / return focus to terminal
      { mods = "LEADER", key = "A",  action = petruterm.action.FocusAiPanel },
      -- leader+a+a : toggle AI panel open / close
      -- leader+a+e : Explain last output
      -- leader+a+f : Fix last error
      -- leader+a+z : Undo last write
      -- (These are handled as hardcoded sub-leader sequences, not config entries.)

      -- ── Explorer sub-leader (leader+e+*) ──────────────────────────────────
      -- leader+e+e : Toggle workspace sidebar

      -- ── Tabs (tmux-style) ─────────────────────────────────────────────────
      { mods = "LEADER", key = "c",  action = petruterm.action.NewTab },
      { mods = "LEADER", key = "&",  action = petruterm.action.CloseTab },
      { mods = "LEADER", key = "n",  action = petruterm.action.NextTab },
      { mods = "LEADER", key = "b",  action = petruterm.action.PrevTab },
      { mods = "LEADER", key = ",",  action = petruterm.action.RenameTab },

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
  else
    config.keys = {
      -- ── Tabs ────────────────────────────────────────────────────────────
      { mods = "CMD",         key = "t", action = petruterm.action.NewTab },
      { mods = "CMD|SHIFT",   key = "W", action = petruterm.action.CloseTab },
      { mods = "CMD",         key = "]", action = petruterm.action.NextTab },
      { mods = "CMD",         key = "[", action = petruterm.action.PrevTab },
      { mods = "CMD|SHIFT",   key = "R", action = petruterm.action.RenameTab },

      -- ── Panes ───────────────────────────────────────────────────────────
      { mods = "CMD",         key = "w", action = petruterm.action.ClosePane },
      { mods = "CMD",         key = "d", action = petruterm.action.SplitVertical },
      { mods = "CMD|SHIFT",   key = "D", action = petruterm.action.SplitHorizontal },
      { mods = "CMD|SHIFT",   key = "M", action = petruterm.action.ZoomPane },

      -- ── Overlays ────────────────────────────────────────────────────────
      { mods = "CMD|SHIFT",   key = "P", action = petruterm.action.CommandPalette },

      -- ── AI controls ─────────────────────────────────────────────────────
      { mods = "CMD|SHIFT",   key = "A", action = petruterm.action.FocusAiPanel },
      { mods = "CMD|OPTION",  key = "a", action = petruterm.action.ToggleAiPanel },
      { mods = "CMD|OPTION",  key = "c", action = petruterm.action.ClearAiContext },
      { mods = "CMD|OPTION",  key = "e", action = petruterm.action.ExplainLastOutput },
      { mods = "CMD|OPTION",  key = "f", action = petruterm.action.FixLastError },
      { mods = "CMD|OPTION",  key = "z", action = petruterm.action.UndoLastWrite },

      -- ── Workspaces ──────────────────────────────────────────────────────
      { mods = "CMD|SHIFT",   key = "N", action = petruterm.action.NewWorkspace },
      { mods = "CMD|SHIFT",   key = "X", action = petruterm.action.CloseWorkspace },
      { mods = "CMD|OPTION",  key = "r", action = petruterm.action.RenameWorkspace },
      { mods = "CMD|SHIFT",   key = "}", action = petruterm.action.NextWorkspace },
      { mods = "CMD|SHIFT",   key = "{", action = petruterm.action.PrevWorkspace },
      { mods = "CMD|SHIFT",   key = "S", action = petruterm.action.SaveWorkspace },
      { mods = "CMD|SHIFT",   key = "O", action = petruterm.action.OpenSavedWorkspaces },
    }
  end
end

return module
