-- PetruTheme Dark for PetruTerm
-- Ported from /Users/edison/tools/PetruTheme (design/palette.md + themes/zed/petru.json):
-- neutrals anchored to Dracula Pro "Van Helsing", 14 Catppuccin-position accents at
-- flat HSL(hue, 95%, 75%). Values taken directly from the Zed theme's own `terminal.*`
-- block (already tuned per-role) rather than re-derived from the raw palette table.
return {
    name          = "PetruTheme Dark",
    foreground    = "#eef0f2",
    background    = "#13171b",
    cursor_bg     = "#fc9783",
    cursor_fg     = "#eef0f2",
    cursor_border = "#fc9783",
    selection_bg  = "#3b4754",
    selection_fg  = "#ccd1d7",
    ansi = {
        "#3b4754", "#fc83a5", "#8dfc83", "#fcd583",
        "#83b1fc", "#fc83dc", "#83fce8", "#ccd1d7",
    },
    brights = {
        "#4c5b6c", "#fdabc2", "#b2fdab", "#fde2ab",
        "#abcafd", "#fdabe7", "#abfdef", "#e3e6e9",
    },
    -- UI tokens
    ui_accent         = "#b983fc",
    ui_surface        = "#0c0e11",
    ui_surface_active = "#3b4754",
    ui_surface_hover  = "#1f262d",
    ui_muted          = "#a4adb7",
    ui_success        = "#8dfc83",
    ui_overlay        = "#0c0e11f2",
    ui_border         = "#2c363f",
}
