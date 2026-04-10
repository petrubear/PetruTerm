-- PetruTerm default configuration
-- This file is the entry point; it composes the module config files.
-- Customize by editing ~/.config/petruterm/config.lua

local ui       = require("ui")
local perf     = require("perf")
local keybinds = require("keybinds")
local llm      = require("llm")

local config = {}

ui.apply_to_config(config)
perf.apply_to_config(config)
keybinds.apply_to_config(config)
llm.apply_to_config(config)

-- Snippets: expand via command palette ("Snippet: …") or via Tab trigger.
-- config.snippets = {
--   { name = "git log pretty",   body = "git log --oneline --graph --decorate --all",  trigger = "gla" },
--   { name = "docker run shell", body = "docker run -it --rm ",                         trigger = "dkr" },
--   { name = "kubectl pods",     body = "kubectl get pods -n ",                          trigger = "kgp" },
-- }

return config
