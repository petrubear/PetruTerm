use anyhow::{anyhow, bail, Result};
use cosmic_text::{fontdb, FontSystem};
use std::collections::HashSet;
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
pub fn build_font_system(
    font_config: &FontConfig,
) -> Result<(FontSystem, String, fontdb::ID, PathBuf, u32)> {
    let locator = FontLocator::new();
    let font_location = match locator.locate_font(&font_config.family) {
        Some(fp) => fp,
        None => bail!(
            "Font '{}' not found in any font directory.\n\
             Make sure the font is installed in ~/Library/Fonts or /Library/Fonts.\n\
             Check the font family name in ~/.config/petruterm/ui.lua.",
            font_config.family
        ),
    };

    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    if let Err(e) = db.load_font_file(&font_location.path) {
        bail!(
            "Font '{}' was found at {:?} but failed to load: {}",
            font_config.family,
            font_location.path,
            e
        );
    }

    // Find the face at the exact index selected by font_kit. FaceInfo.index is the
    // face index within the source file (0 for .ttf; 0+ for .ttc collections).
    // Matching on both path and index avoids picking the wrong face from a collection.
    let face = db
        .faces()
        .find(|face| {
            let path_ok = match &face.source {
                fontdb::Source::File(path) => path == &font_location.path,
                fontdb::Source::SharedFile(path, _) => path == &font_location.path,
                _ => false,
            };
            path_ok && face.index == font_location.index
        })
        .or_else(|| {
            // Fallback: path-only match for fonts where index is unavailable.
            db.faces().find(|face| match &face.source {
                fontdb::Source::File(path) => path == &font_location.path,
                fontdb::Source::SharedFile(path, _) => path == &font_location.path,
                _ => false,
            })
        })
        .ok_or_else(|| {
            anyhow!(
                "No faces found for selected font file {:?}",
                font_location.path
            )
        })?;

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
    let face_index = face.index;

    log::info!(
        "Font loaded: internal family='{}' (config='{}') {}pt from {:?} (face {})",
        actual_family,
        font_config.family,
        font_config.size,
        font_location.path,
        face_index,
    );

    let mut font_system = FontSystem::new_with_locale_and_db("en-US".to_string(), db);
    register_variable_weights(&mut font_system, &actual_family);
    Ok((
        font_system,
        actual_family,
        face_id,
        font_location.path,
        face_index,
    ))
}

/// Weights offered for variable faces.
const VARIABLE_WEIGHTS: [u16; 9] = [100, 200, 300, 400, 500, 600, 700, 800, 900];

/// Base family of a Nerd Font patch: "MonoLisaCode Nerd Font Mono" -> "MonoLisaCode".
fn base_family(family: &str) -> Option<&str> {
    let i = family.to_ascii_lowercase().find(" nerd font")?;
    Some(&family[..i])
}

/// Makes variable-weight faces selectable at every weight in their `wght` range,
/// under `family`.
///
/// cosmic-text only selects a face whose fontdb weight equals the requested one,
/// and fontdb registers a variable font once, at its default weight -- so a Bold
/// request skips a variable family and falls through to the platform fallback
/// (Menlo on macOS). Registering extra faces for the same file lets the request
/// match, and cosmic-text then applies the `wght` axis itself.
///
/// Sources are `family`'s own variable faces plus those of its non-Nerd-Font
/// base family: Nerd Font patches are usually static (no bold), so bold has to
/// come from the base font. A (style, weight) that `family` already provides is
/// never replaced, so the patched regular/italic faces (with their icons) stay.
fn register_variable_weights(font_system: &mut FontSystem, family: &str) {
    let names: Vec<&str> = std::iter::once(family).chain(base_family(family)).collect();
    let in_family = |f: &fontdb::FaceInfo, name: &str| {
        f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(name))
    };
    let sources: Vec<fontdb::FaceInfo> = font_system
        .db()
        .faces()
        .filter(|f| names.iter().any(|n| in_family(f, n)))
        .cloned()
        .collect();
    let mut provided: HashSet<(fontdb::Style, u16)> = font_system
        .db()
        .faces()
        .filter(|f| in_family(f, family))
        .map(|f| (f.style, f.weight.0))
        .collect();

    for src in sources {
        let Some(font) = font_system.get_font(src.id, src.weight) else {
            continue;
        };
        let Some((min, max)) = font
            .as_swash()
            .variations()
            .find_by_tag(u32::from_be_bytes(*b"wght"))
            .map(|v| (v.min_value(), v.max_value()))
        else {
            continue;
        };
        for weight in VARIABLE_WEIGHTS {
            if !(min..=max).contains(&f32::from(weight)) || !provided.insert((src.style, weight)) {
                continue;
            }
            let mut info = src.clone();
            info.id = fontdb::ID::dummy();
            info.weight = fontdb::Weight(weight);
            info.families = vec![(family.to_string(), fontdb::Language::English_UnitedStates)];
            font_system.db_mut().push_face_info(info);
        }
    }
}

/// Locates the user-selected font for LCD AA and sets font_path in the config.
static FONT_PATH_CACHE: std::sync::OnceLock<
    parking_lot::Mutex<std::collections::HashMap<String, Option<PathBuf>>>,
> = std::sync::OnceLock::new();

pub fn locate_font_for_lcd(font_config: &mut FontConfig) {
    if !font_config.lcd_antialiasing {
        return;
    }

    let cache =
        FONT_PATH_CACHE.get_or_init(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

    {
        let cache = cache.lock();
        if let Some(cached_path) = cache.get(&font_config.family) {
            font_config.font_path = cached_path.clone();
            return;
        }
    }

    let locator = FontLocator::new();
    let path = locator.locate_font(&font_config.family).map(|p| p.path);
    cache
        .lock()
        .insert(font_config.family.clone(), path.clone());
    font_config.font_path = path;
}

#[cfg(test)]
mod tests {
    use super::base_family;

    #[test]
    fn strips_nerd_font_suffix() {
        assert_eq!(base_family("MonoLisaCode Nerd Font"), Some("MonoLisaCode"));
        assert_eq!(
            base_family("JetBrainsMono Nerd Font Mono"),
            Some("JetBrainsMono")
        );
        assert_eq!(base_family("Menlo"), None);
    }
}
