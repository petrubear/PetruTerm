use anyhow::Result;
use cosmic_text::{fontdb, FontSystem, SwashCache};
use std::collections::HashSet;

use crate::config::schema::FontConfig;
use crate::font::locator::FontLocator;

// Bundled JetBrains Mono Nerd Font Mono (v3.3.0) — embedded at compile time.
// The "Mono" variant ensures Nerd Font icons are single-cell-width in terminal grids.
// Source: https://github.com/ryanoasis/nerd-fonts/releases/tag/v3.3.0
const JBM_REGULAR: &[u8] =
    include_bytes!("../../assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf");
const JBM_BOLD: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFontMono-Bold.ttf");
const JBM_ITALIC: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFontMono-Italic.ttf");
const JBM_BOLD_ITALIC: &[u8] =
    include_bytes!("../../assets/fonts/JetBrainsMonoNerdFontMono-BoldItalic.ttf");

/// Initializes a focused cosmic-text FontSystem with only the necessary fonts.
pub fn build_font_system(font_config: &FontConfig) -> Result<FontSystem> {
    let mut db = fontdb::Database::new();

    // 1. Load bundled JetBrains Mono — always available as a base.
    db.load_font_data(JBM_REGULAR.to_vec());
    db.load_font_data(JBM_BOLD.to_vec());
    db.load_font_data(JBM_ITALIC.to_vec());
    db.load_font_data(JBM_BOLD_ITALIC.to_vec());

    // 2. Load fonts from the user's custom font directory.
    let user_font_dir = crate::config::config_dir().join("fonts");
    if user_font_dir.exists() {
        db.load_fonts_dir(&user_font_dir);
    }

    // 3. Explicitly locate and load the primary font and user/system fallbacks.
    // This avoids a full system scan and is much more performant.
    let locator = FontLocator::new();
    let mut loaded_families = HashSet::new();

    let families_to_load = std::iter::once(font_config.family.as_str())
        .chain(font_config.fallbacks.iter().map(String::as_str))
        .chain(["Menlo", "SF Mono", "Monaco", "Courier New"]); // Generic fallbacks

    for family in families_to_load {
        if loaded_families.contains(family) {
            continue;
        }
        if let Some(font_path) = locator.locate_font(family) {
            if db.load_font_file(&font_path.path).is_ok() {
                log::debug!("Loaded font file: {:?}", font_path.path);
                loaded_families.insert(family.to_string());
            } else {
                log::warn!("Failed to load font file: {:?}", font_path.path);
            }
        }
    }

    // Construct the FontSystem with our curated database.
    // We pass a default locale; cosmic-text uses this for language-specific shaping.
    let font_system = FontSystem::new_with_locale_and_db("en-US".to_string(), db);

    log::info!(
        "Font system initialized. Primary: '{}' {}pt",
        font_config.family,
        font_config.size
    );

    // Check if the primary font was successfully loaded into the database.
    let primary_font_missing = !font_system
        .db()
        .faces()
        .any(|face| face.families.iter().any(|(f, _)| f == &font_config.family));

    if primary_font_missing {
        log::warn!(
            "Primary font '{}' not found or failed to load. 
            Please ensure it's installed correctly. 
            A fallback font will be used.",
            font_config.family
        );
    }

    Ok(font_system)
}

/// Cached font path lookup to avoid repeated filesystem scans.
static FONT_PATH_CACHE: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, Option<std::path::PathBuf>>>,
> = std::sync::OnceLock::new();

/// Locates a font for LCD AA and populates font_path in the config.
/// For bundled fonts (JetBrainsMono Nerd Font), returns bundled bytes.
/// For other fonts, uses CoreText via font-kit to locate the system font file path.
/// Result is cached per family to avoid repeated lookups.
pub fn locate_font_for_lcd(font_config: &mut FontConfig) {
    if !font_config.lcd_antialiasing {
        return;
    }

    // First check if it's a bundled JetBrainsMono Nerd Font
    if font_config.family.contains("JetBrainsMono") && font_config.family.contains("Nerd Font") {
        font_config.bundled_font_data = Some(JBM_REGULAR.to_vec());
        log::info!(
            "LCD AA: using bundled JetBrainsMono Nerd Font for '{}'",
            font_config.family
        );
        return;
    }

    // Get or create cache
    let cache =
        FONT_PATH_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));

    {
        let cache = cache.lock().unwrap();
        if let Some(cached_path) = cache.get(&font_config.family) {
            font_config.font_path = cached_path.clone();
            return;
        }
    }

    // Locate and cache
    let locator = FontLocator::new();
    let path = locator.locate_font(&font_config.family).map(|p| p.path);
    cache
        .lock()
        .unwrap()
        .insert(font_config.family.clone(), path.clone());
    font_config.font_path = path;
}
