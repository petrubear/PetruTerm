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

	-- Line height multiplier (1.0 = no extra leading, 1.4 = 40% extra spacing).
	config.font_line_height = 1.4

	-- HarfBuzz OpenType features: contextual alternates, ligatures, discretionary ligatures.
	config.font_features = { "calt=1", "liga=1", "dlig=1" }

	-- Fallback font families tried in order when a glyph is not found.
	config.font_fallbacks = { "Apple Color Emoji", "Noto Color Emoji" }

	-- Enable LCD subpixel antialiasing (FreeType LCD mode). Only effective on Linux/X11.
	config.lcd_antialiasing = false

	-- ── Color scheme (Dracula Pro) ───────────────────────────────────────────
	config.colors = {
		foreground    = "#e0e0e8",
		background    = "#0e0e10",
		cursor_bg     = "#9580ff",
		cursor_border = "#9580ff",
		cursor_fg     = "#e0e0e8",
		selection_bg  = "#2a2a3a",
		selection_fg  = "#e0e0e8",
		ansi    = { "#0e0e10", "#ff9580", "#8aff80", "#ffff80", "#9580ff", "#ff80bf", "#80ffea", "#e0e0e8" },
		brights = { "#2a2a2f", "#ffaa99", "#a2ff99", "#ffff99", "#aa99ff", "#ff99cc", "#99ffee", "#ffffff" },

		-- Semantic UI tokens (optional — derived from base colors when omitted).
		-- ui_accent:         focus borders, highlights.       Default: cursor_bg.
		-- ui_surface:        panel / sidebar / palette bg.    Default: background +15% brightness.
		-- ui_surface_active: selected item bg.                Default: selection_bg.
		-- ui_surface_hover:  hovered item bg.                 Default: background +8% brightness.
		-- ui_muted:          separators, secondary text.      Default: foreground at 35% alpha.
		-- ui_success:        positive indicators.             Default: ansi[3] (green).
		-- ui_overlay:        toast / modal semi-transparent.  Default: background at 95% alpha.
		--   Supports 6-char (#rrggbb) or 8-char (#rrggbbaa) hex values.
		-- ui_accent         = "#9580ff",
		-- ui_surface        = "#131316",
		-- ui_surface_active = "#2a2a3a",
		-- ui_surface_hover  = "#181818",
		-- ui_muted          = "#e0e0e859",
		-- ui_success        = "#8aff80",
		-- ui_overlay        = "#131316f2",
	}

	-- ── Window ───────────────────────────────────────────────────────────────
	-- title_bar_style:
	--   "custom" — transparent title bar, traffic lights in native position,
	--              content extends behind bar (macOS only).
	--   "native" — standard OS title bar.
	--   "none"   — fully borderless (no chrome at all).
	config.window = {
		borderless      = false,
		-- Set initial_width / initial_height to override the default 1280×800 startup size.
		-- initial_width  = 1440,
		-- initial_height = 900,
		start_maximized = true,
		title_bar_style = "custom",
		-- top is the gap between the titlebar and the first terminal row.
		-- The titlebar height (30 px) is handled internally — do not add it here.
		padding = { left = 20, right = 20, top = 5, bottom = 10 },
		-- Window background opacity (0.0 = fully transparent, 1.0 = opaque).
		opacity = 1.0,
	}

	config.enable_tab_bar     = true
	config.hide_tab_bar_if_one = true

	-- ── Input decoration ─────────────────────────────────────────────────────
	-- Colorize the command as you type: green/red for command, cyan for flags, yellow for strings.
	-- Set to false if you use zsh-syntax-highlighting.
	config.input_syntax_highlight = true

	-- Show ghost text (history-based inline completion) after the cursor while typing.
	-- Set to false if you use zsh-autosuggestions or fish — they already provide this,
	-- and having both active causes conflicts (double text written to shell on ArrowRight).
	config.input_ghost_text = true

	-- ── Status bar ───────────────────────────────────────────────────────────
	-- enabled:  show/hide the status bar (also togglable via command palette).
	-- position: "bottom" (default) or "top".
	-- style:    "plain"     — text separators ( › and │ ).
	--           "powerline" — Nerd Font arrows ( and ). Requires a Nerd Font.
	config.status_bar = {
		enabled  = true,
		position = "bottom",
		style    = "plain",
	}
end

return module
