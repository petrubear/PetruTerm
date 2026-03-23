use anyhow::Result;
use cosmic_text::{FontSystem, SwashCache};

use crate::config::schema::FontConfig;

// Bundled JetBrains Mono Nerd Font Mono (v3.3.0) — embedded at compile time.
// The "Mono" variant ensures Nerd Font icons are single-cell-width in terminal grids.
// Source: https://github.com/ryanoasis/nerd-fonts/releases/tag/v3.3.0
const JBM_REGULAR:     &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf");
const JBM_BOLD:        &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFontMono-Bold.ttf");
const JBM_ITALIC:      &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFontMono-Italic.ttf");
const JBM_BOLD_ITALIC: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFontMono-BoldItalic.ttf");

/// Initializes the cosmic-text FontSystem with bundled + system fonts.
pub fn build_font_system(font_config: &FontConfig) -> Result<FontSystem> {
    // FontSystem::new() scans system fonts automatically.
    let mut font_system = FontSystem::new();

    // Load bundled JetBrains Mono — always available regardless of system.
    {
        let db = font_system.db_mut();
        db.load_font_data(JBM_REGULAR.to_vec());
        db.load_font_data(JBM_BOLD.to_vec());
        db.load_font_data(JBM_ITALIC.to_vec());
        db.load_font_data(JBM_BOLD_ITALIC.to_vec());
    }

    // Load any additional fonts from the user's PetruTerm font directory.
    let user_font_dir = crate::config::config_dir().join("fonts");
    if user_font_dir.exists() {
        load_fonts_from_dir(&mut font_system, &user_font_dir);
    }

    log::info!(
        "Font system initialized. Primary: '{}' {}pt",
        font_config.family, font_config.size
    );

    // Verify the primary font is available; warn and suggest alternatives if not.
    if !font_available(&font_system, &font_config.family) {
        // Try user-configured fallbacks, then known macOS system fonts.
        let system_fallbacks = ["Menlo", "SF Mono", "Monaco", "Courier New"];
        let all_fallbacks: Vec<&str> = font_config.fallbacks.iter()
            .map(String::as_str)
            .chain(system_fallbacks.iter().copied())
            .collect();

        let found = all_fallbacks.iter().find(|&&f| font_available(&font_system, f));
        match found {
            Some(fb) => log::warn!(
                "Font '{}' not found. Using fallback: '{fb}'. \
                 Install '{}' or set `font.family` in config.lua.",
                font_config.family, font_config.family
            ),
            None => log::warn!(
                "Font '{}' not found and no fallbacks available. \
                 Text may render with a default system font.",
                font_config.family
            ),
        }
    }

    Ok(font_system)
}

/// Load all .ttf / .otf / .ttc files from a directory into the font system.
fn load_fonts_from_dir(font_system: &mut FontSystem, dir: &std::path::Path) {
    font_system.db_mut().load_fonts_dir(dir);
    log::debug!("Loaded user fonts from: {}", dir.display());
}

/// Check if a font family name is available in the font system.
fn font_available(font_system: &FontSystem, family: &str) -> bool {
    font_system
        .db()
        .faces()
        .any(|f| f.families.iter().any(|(name, _)| name.eq_ignore_ascii_case(family)))
}

/// Construct a SwashCache for glyph rasterization.
pub fn build_swash_cache() -> SwashCache {
    SwashCache::new()
}
