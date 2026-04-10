-- PetruTerm default configuration
-- This file is the entry point; it composes the module config files.
-- Customize by editing ~/.config/petruterm/config.lua

local ui       = require("ui")
local perf     = require("perf")
local keybinds = require("keybinds")
local llm      = require("llm")
local snippets = require("snippets")

local config = {}

ui.apply_to_config(config)
perf.apply_to_config(config)
keybinds.apply_to_config(config)
llm.apply_to_config(config)
snippets.apply_to_config(config)

return config
