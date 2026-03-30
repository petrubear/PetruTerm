use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FontPath {
    pub path: PathBuf,
    pub index: u32,
}

pub struct FontLocator;

impl FontLocator {
    pub fn new() -> Self {
        Self
    }

    pub fn locate_font(&self, family: &str) -> Option<FontPath> {
        // Try system source first
        if let Some(path) = self.locate_via_font_kit(family) {
            return Some(path);
        }

        // Scan ~/Library/Fonts directly
        self.scan_user_fonts(family)
    }

    fn locate_via_font_kit(&self, family: &str) -> Option<FontPath> {
        let source = font_kit::source::SystemSource::new();
        let families = source.all_families().ok()?;

        // Try exact match first
        let matched = families.iter().find(|f| f.eq_ignore_ascii_case(family))?;

        let family_handle = source.select_family_by_name(matched).ok()?;
        let handles = family_handle.fonts();

        // Collect all handles with their weights
        let mut regular: Option<FontPath> = None;
        let mut medium: Option<FontPath> = None;
        let mut others: Vec<FontPath> = Vec::new();

        for h in handles {
            if let Ok(font) = h.load() {
                let path = match font.handle()? {
                    font_kit::handle::Handle::Path { path, .. } => path.clone(),
                    font_kit::handle::Handle::Memory { .. } => continue,
                };
                let weight = font.properties().weight.0 as i32;

                let fp = FontPath { path, index: 0 };
                if weight == 400 {
                    regular = Some(fp);
                } else if weight == 500 {
                    medium = Some(fp);
                } else {
                    others.push(fp);
                }
            }
        }

        // Prefer Regular (400) > Medium (500) > others
        regular.or(medium).or_else(|| others.into_iter().next())
    }

    fn scan_user_fonts(&self, family: &str) -> Option<FontPath> {
        let user_fonts = dirs::home_dir()?.join("Library/Fonts");
        if !user_fonts.exists() {
            return None;
        }

        let entries = std::fs::read_dir(user_fonts).ok()?;
        let family_lower = family.to_lowercase();
        let family_nospace = family_lower.replace(' ', "");

        let mut candidates: Vec<(i32, PathBuf)> = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension()?.to_str()?;
            if ext != "ttf" && ext != "otf" {
                continue;
            }

            let filename = path.file_name()?.to_string_lossy().to_lowercase();

            // Check if filename matches family name
            if !filename.contains(&family_nospace) {
                continue;
            }

            // Score by preference (lower is better)
            let score = if filename.contains("regular") && !filename.contains("bold") {
                0
            } else if filename.contains("medium") && !filename.contains("bold") {
                1
            } else if !filename.contains("bold") && !filename.contains("italic") {
                2
            } else {
                3
            };

            candidates.push((score, path));
        }

        candidates.sort_by_key(|(s, _)| *s);
        candidates
            .into_iter()
            .next()
            .map(|(_, p)| FontPath { path: p, index: 0 })
    }
}

impl Default for FontLocator {
    fn default() -> Self {
        Self::new()
    }
}
