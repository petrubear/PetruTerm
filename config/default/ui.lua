-- PetruTerm UI configuration
-- Controls font, colors, window appearance.

local petruterm = require("petruterm")
local module = {}

function module.apply_to_config(config)
	-- ── Font ─────────────────────────────────────────────────────────────────
	-- Primary font family. petruterm.font() resolves the first installed family
	-- from a comma-separated priority list.
	config.font = petruterm.font("MonolisaCode Nerd Font, JetBrainsMono Nerd Font Mono, Monolisa Nerd Font, Fira Code, Menlo")

	-- Font size in points.
	config.font_size = 16

	-- Line height multiplier (1.0 = no extra leading, 1.4 = 40% extra spacing).
	config.font_line_height = 1.4

	-- OpenType features: contextual alternates, ligatures, discretionary ligatures.
	config.font_features = { "calt=1", "liga=1", "dlig=1" }

	-- FreeType LCD subpixel antialiasing (wgpu binary only).
	config.lcd_antialiasing = true

	-- ── Color scheme (PetruTheme Dark) ───────────────────────────────────────────
	config.colors = {
		foreground    = "#eef0f2",
		background    = "#13171b",
		cursor_bg     = "#fc9783",
		cursor_border = "#fc9783",
		cursor_fg     = "#eef0f2",
		selection_bg  = "#3b4754",
		selection_fg  = "#ccd1d7",
		ansi    = { "#3b4754", "#fc83a5", "#8dfc83", "#fcd583", "#83b1fc", "#fc83dc", "#83fce8", "#ccd1d7" },
		brights = { "#4c5b6c", "#fdabc2", "#b2fdab", "#fde2ab", "#abcafd", "#fdabe7", "#abfdef", "#e3e6e9" },

		-- Semantic UI tokens (optional — derived from base colors when omitted).
		-- ui_accent:         focus borders, highlights.       Default: cursor_bg.
		-- ui_surface:        panel / sidebar / palette bg.    Default: background +15% brightness.
		-- ui_surface_active: selected item bg.                Default: selection_bg.
		-- ui_surface_hover:  hovered item bg.                 Default: background +8% brightness.
		-- ui_muted:          separators, secondary text.      Default: foreground at 35% alpha.
		-- ui_success:        positive indicators.             Default: ansi[3] (green).
		-- ui_overlay:        toast / modal semi-transparent.  Default: background at 95% alpha.
		-- ui_border:         pane separators, card outlines.  Default: background +17% brightness.
		--   Supports 6-char (#rrggbb) or 8-char (#rrggbbaa) hex values.
		ui_accent         = "#b983fc",
		ui_surface        = "#0c0e11",
		ui_surface_active = "#3b4754",
		ui_surface_hover  = "#1f262d",
		ui_muted          = "#a4adb7",
		ui_success        = "#8dfc83",
		ui_overlay        = "#0c0e11f2",
		-- ui_border      = "#33333f",
	}

	-- ── Window ───────────────────────────────────────────────────────────────
	-- title_bar_style:
	--   "custom" — transparent title bar, traffic lights in native position,
	--              content extends behind bar (macOS only).
	--   "native" — standard OS title bar.
	--   "none"   — fully borderless (no chrome at all).
	config.window = {
		-- Set initial_width / initial_height to override the default 1280×800 startup size.
		-- initial_width  = 1440,
		-- initial_height = 900,
		start_maximized = true,
		title_bar_style = "custom",
		-- top is the gap between the titlebar and the first terminal row.
		-- The titlebar height (30 px) is handled internally — do not add it here.
		padding = { left = 10, right = 10, top = 5, bottom = 5 },
		-- Window background opacity (0.0 = fully transparent, 1.0 = opaque).
		opacity = 1.0,
		-- macOS vibrancy/blur behind the window content.
		--   false (or omit) = disabled; "dark" or "light" = NSVisualEffectView material.
		-- For the blur to be visible through the terminal, also lower `opacity`
		-- (e.g. 0.85). With blur on and opacity = 1.0, a 0.82 fallback is used.
		-- blur = "dark",
	}

	-- ── Input decoration ─────────────────────────────────────────────────────
	-- Colorize the command as you type: green/red for command, cyan for flags, yellow for strings.
	-- Set to false if you use zsh-syntax-highlighting.
	config.input_syntax_highlight = false

	-- Show ghost text (history-based inline completion) after the cursor while typing.
	-- Set to false if you use zsh-autosuggestions or fish — they already provide this,
	-- and having both active causes conflicts (double text written to shell on ArrowRight).
	config.input_ghost_text = false

	-- ── Status bar ───────────────────────────────────────────────────────────
	-- enabled:  show/hide the status bar (also togglable via command palette).
	-- position: "bottom" (default) or "top".
	-- style:    "plain"     — text separators ( › and │ ).
	--           "powerline" — Nerd Font arrows ( and ). Requires a Nerd Font.
	config.status_bar = {
		enabled  = true,
		position = "bottom",
		style    = "powerline",
	}
end

return module
