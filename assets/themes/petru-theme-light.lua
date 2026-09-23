-- PetruTheme Light for PetruTerm
-- Ported from PetruTheme (palette.md + Zed petru.json):
-- neutrals anchored to Dracula Pro "Alucard", 14 accents per-hue-tuned to land at
-- ~4.7:1 contrast against Base. Values taken directly from the Zed theme's own
-- `terminal.*` block (already tuned per-role) rather than re-derived from the raw
-- palette table.
return {
    name          = "PetruTheme Light",
    foreground    = "#1d1d20",
    background    = "#f7f7f8",
    cursor_bg     = "#d32e0d",
    cursor_fg     = "#1d1d20",
    cursor_border = "#d32e0d",
    selection_bg  = "#d1d0dd",
    selection_fg  = "#353347",
    ansi = {
        "#b4b2c7", "#db0e48", "#128108", "#936709",
        "#1166f0", "#cf0d9b", "#087d6a", "#353347",
    },
    brights = {
        "#9d9ab6", "#b50c3b", "#0d5b06", "#6d4c07",
        "#0d55cb", "#a90b7e", "#065749", "#23222f",
    },
    -- UI tokens
    ui_accent         = "#903ef3",
    ui_surface        = "#efeff1",
    ui_surface_active = "#d1d0dd",
    ui_surface_hover  = "#dcdce5",
    ui_muted          = "#504d6a",
    ui_success        = "#128108",
    ui_overlay        = "#efeff1f2",
    ui_border         = "#d1d0dd",
}
