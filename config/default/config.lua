-- PetruTerm default configuration
-- This file is the entry point; it composes the module config files.
-- Customize by editing ~/.config/petruterm/config.lua

local ui            = require("ui")
local perf          = require("perf")
local keybinds      = require("keybinds")
local llm           = require("llm")
local snippets      = require("snippets")
local notifications = require("notifications")

local config = {}

ui.apply_to_config(config)
perf.apply_to_config(config)
keybinds.apply_to_config(config)
llm.apply_to_config(config)
snippets.apply_to_config(config)
notifications.apply_to_config(config)

-- Shell to launch in new tabs. Defaults to $SHELL or /bin/zsh.
config.shell             = os.getenv("SHELL") or "/bin/zsh"

-- Inject shell integration (CWD tracking, exit codes, last command).
config.shell_integration = true

config.workspaces = {
  -- Save the current workspace layout when the app quits.
  auto_save_on_exit = true,
}

return config
