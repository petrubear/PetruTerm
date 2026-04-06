-- PetruTerm UI configuration
-- Controls font, colors, window appearance.

local petruterm = require("petruterm")
local module = {}

function module.apply_to_config(config)
	-- ── Font ─────────────────────────────────────────────────────────────────
	-- Primary font family. petruterm.font() resolves the first installed family
	-- from a comma-separated priority list.
	config.font = petruterm.font("JetBrainsMono Nerd Font Mono, Monolisa Nerd Font, Fira Code, Menlo")

	-- Font size in points.
	config.font_size = 16

	-- Line height multiplier (1.0 = no extra leading, 1.2 = 20% extra spacing).
	config.font_line_height = 1.2

	-- HarfBuzz OpenType features: contextual alternates, ligatures, discretionary ligatures.
	config.font_features = { "calt=1", "liga=1", "dlig=1" }

	-- Fallback font families tried in order when a glyph is not found.
	config.font_fallbacks = { "Apple Color Emoji", "Noto Color Emoji" }

	-- Enable LCD subpixel antialiasing (FreeType LCD mode). Only effective on Linux/X11.
	config.lcd_antialiasing = false

	-- ── Color scheme (Dracula Pro) ───────────────────────────────────────────
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

	-- ── Window ───────────────────────────────────────────────────────────────
	-- title_bar_style:
	--   "custom" — transparent title bar, traffic lights in native position,
	--              content extends behind bar (macOS only). top padding >= 60.
	--   "native" — standard OS title bar.
	--   "none"   — fully borderless (no chrome at all).
	config.window = {
		borderless      = false,
		-- Set initial_width / initial_height to override the default 1280×800 startup size.
		-- initial_width  = 1440,
		-- initial_height = 900,
		start_maximized = true,
		title_bar_style = "custom",
		-- top should be >= 60 when using "custom" to clear the traffic lights on macOS.
		padding = { left = 20, right = 20, top = 60, bottom = 10 },
		-- Window background opacity (0.0 = fully transparent, 1.0 = opaque).
		opacity = 1.0,
	}

	config.enable_tab_bar     = true
	config.hide_tab_bar_if_one = true
end

return module
