use anyhow::{anyhow, bail, Result};
use cosmic_text::{fontdb, FontSystem};
use std::path::PathBuf;

use crate::config::schema::FontConfig;
use crate::font::locator::FontLocator;

/// Initializes a cosmic-text FontSystem with the system font database plus the
/// user-selected primary font.
/// Returns:
///   - FontSystem  — cosmic-text engine
///   - String      — actual internal family name (queried from fontdb, may differ from config)
///   - fontdb::ID  — fontdb face ID (needed to build CacheKeys for PUA glyph override)
///   - PathBuf     — resolved font file path (for FreeType cmap lookup)
pub fn build_font_system(font_config: &FontConfig) -> Result<(FontSystem, String, fontdb::ID, PathBuf)> {
    let locator = FontLocator::new();
    let font_path = match locator.locate_font(&font_config.family) {
        Some(fp) => fp.path,
        None => bail!(
            "Font '{}' not found in any font directory.\n\
             Make sure the font is installed in ~/Library/Fonts or /Library/Fonts.\n\
             Check the font family name in ~/.config/petruterm/ui.lua.",
            font_config.family
        ),
    };

    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    if let Err(e) = db.load_font_file(&font_path) {
        bail!(
            "Font '{}' was found at {:?} but failed to load: {}",
            font_config.family,
            font_path,
            e
        );
    }

    // Read the actual internal family name and face ID for the selected file.
    // The internal name may differ from the config string (e.g. "MonoLisa Nerd Font"
    // vs "Monolisa Nerd Font"). Using the family from the chosen file ensures the
    // primary face is selected first, while the rest of the system database remains
    // available for fallback glyphs that the primary font does not contain.
    let face = db
        .faces()
        .find(|face| match &face.source {
            fontdb::Source::File(path) => path == &font_path,
            fontdb::Source::SharedFile(path, _) => path == &font_path,
            _ => false,
        })
        .ok_or_else(|| anyhow!("No faces found for selected font file {:?}", font_path))?;

    // Prioritize the family name that matches the config exactly, or one that contains "Mono"
    let actual_family: String = face
        .families
        .iter()
        .find(|(name, _)| name.to_lowercase() == font_config.family.to_lowercase())
        .or_else(|| face.families.iter().find(|(name, _)| name.contains("Mono")))
        .or_else(|| face.families.first())
        .map(|(name, _)| name.clone())
        .unwrap_or_else(|| font_config.family.clone());

    let face_id = face.id;

    log::info!(
        "Font loaded: internal family='{}' (config='{}') {}pt from {:?}",
        actual_family,
        font_config.family,
        font_config.size,
        font_path,
    );

    let font_system = FontSystem::new_with_locale_and_db("en-US".to_string(), db);
    Ok((font_system, actual_family, face_id, font_path))
}

/// Locates the user-selected font for LCD AA and sets font_path in the config.
static FONT_PATH_CACHE: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, Option<PathBuf>>>,
> = std::sync::OnceLock::new();

pub fn locate_font_for_lcd(font_config: &mut FontConfig) {
    if !font_config.lcd_antialiasing {
        return;
    }

    let cache =
        FONT_PATH_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));

    {
        let cache = cache.lock().unwrap();
        if let Some(cached_path) = cache.get(&font_config.family) {
            font_config.font_path = cached_path.clone();
            return;
        }
    }

    let locator = FontLocator::new();
    let path = locator.locate_font(&font_config.family).map(|p| p.path);
    cache
        .lock()
        .unwrap()
        .insert(font_config.family.clone(), path.clone());
    font_config.font_path = path;
}
