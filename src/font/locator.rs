use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct FontPath {
    pub path: PathBuf,
    pub index: u32,
}

pub struct FontLocator;

/// Standard macOS font directories, in search priority order.
const MACOS_FONT_DIRS: &[&str] = &[
    // User-installed fonts (highest priority)
    "~/Library/Fonts",
    // System-wide installed fonts
    "/Library/Fonts",
    // macOS built-in fonts
    "/System/Library/Fonts",
    "/System/Library/Fonts/Supplemental",
];

impl FontLocator {
    pub fn new() -> Self {
        Self
    }

    pub fn locate_font(&self, family: &str) -> Option<FontPath> {
        // Try system source first (CoreText, fast)
        if let Some(path) = self.locate_via_font_kit(family) {
            return Some(path);
        }

        // Scan all standard macOS font directories
        self.scan_font_dirs(family)
    }

    fn locate_via_font_kit(&self, family: &str) -> Option<FontPath> {
        use font_kit::family_name::FamilyName;
        use font_kit::properties::Properties;

        let source = font_kit::source::SystemSource::new();

        let handle = source
            .select_best_match(
                &[FamilyName::Title(family.to_owned())],
                &Properties::new(),
            )
            .ok()?;

        match handle {
            font_kit::handle::Handle::Path { path, font_index } => Some(FontPath { path, index: font_index }),
            font_kit::handle::Handle::Memory { .. } => None,
        }
    }

    fn scan_font_dirs(&self, family: &str) -> Option<FontPath> {
        let home = dirs::home_dir().unwrap_or_default();
        // Split family into words and lowercase for matching.
        // "MonoLisa Nerd Font" → ["monolisa", "nerd", "font"]
        let family_words: Vec<String> = family.split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();

        let dirs: Vec<PathBuf> = MACOS_FONT_DIRS
            .iter()
            .map(|d| {
                let s = d.replace('~', home.to_str().unwrap_or(""));
                PathBuf::from(s)
            })
            .filter(|p| p.exists())
            .collect();

        let mut candidates: Vec<(i32, PathBuf)> = Vec::new();

        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                    continue;
                };
                if ext != "ttf" && ext != "otf" {
                    continue;
                }
                let stem = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();

                // All words from the family name must appear in the filename stem.
                if !family_words.iter().all(|w| stem.contains(w.as_str())) {
                    continue;
                }

                let score = if stem.contains("regular") && !stem.contains("bold") {
                    0
                } else if stem.contains("medium") && !stem.contains("bold") {
                    1
                } else if !stem.contains("bold") && !stem.contains("italic") {
                    2
                } else {
                    3
                };
                candidates.push((score, path));
            }
        }

        candidates.sort_by_key(|(s, _)| *s);
        candidates.into_iter().next().map(|(_, p)| FontPath { path: p, index: 0 })
    }
}

impl Default for FontLocator {
    fn default() -> Self {
        Self::new()
    }
}
