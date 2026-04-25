use std::path::{Path, PathBuf};

pub struct SteeringManager {
    files: Vec<(String, String)>,
}

impl SteeringManager {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    /// Load `*.md` files from `~/.config/petruterm/steering/` (global) and
    /// `<cwd>/.petruterm/steering/` (project-local, wins on name clash).
    pub fn load(&mut self, cwd: &Path) {
        self.files.clear();

        if let Some(home) = dirs::home_dir() {
            let global = home.join(".config/petruterm/steering");
            self.scan_dir(&global);
        }

        let local = cwd.join(".petruterm/steering");
        self.scan_dir_overlay(&local);
    }

    #[allow(dead_code)]
    pub fn count(&self) -> usize {
        self.files.len()
    }

    pub fn files(&self) -> &[(String, String)] {
        &self.files
    }

    /// Build a context block to inject into the system prompt.
    /// Returns `None` when no files are loaded.
    pub fn context_block(&self) -> Option<String> {
        if self.files.is_empty() {
            return None;
        }
        let mut out = String::from(
            "The following steering instructions are always active. \
             Follow them throughout this entire conversation:\n",
        );
        for (name, content) in &self.files {
            out.push_str(&format!("\n--- {name} ---\n{content}"));
        }
        Some(out)
    }

    // ── private ───────────────────────────────────────────────────────────────

    fn scan_dir(&mut self, dir: &Path) {
        let mut entries = read_md_files(dir);
        entries.sort_by(|(a, _), (b, _)| a.cmp(b));
        self.files.extend(entries);
    }

    /// Like `scan_dir` but project files replace globals with the same name.
    fn scan_dir_overlay(&mut self, dir: &Path) {
        let entries = read_md_files(dir);
        for (name, content) in entries {
            if let Some(pos) = self.files.iter().position(|(n, _)| *n == name) {
                self.files[pos] = (name, content);
            } else {
                self.files.push((name, content));
            }
        }
        self.files.sort_by(|(a, _), (b, _)| a.cmp(b));
    }
}

fn read_md_files(dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = Vec::new();
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension() else {
            continue;
        };
        if ext != "md" {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            out.push((name.to_string(), content));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_block_empty() {
        let mgr = SteeringManager::new();
        assert!(mgr.context_block().is_none());
    }

    #[test]
    fn context_block_with_files() {
        let mut mgr = SteeringManager::new();
        mgr.files.push(("rules.md".into(), "Be concise.".into()));
        let block = mgr.context_block().unwrap();
        assert!(block.contains("steering instructions"));
        assert!(block.contains("--- rules.md ---"));
        assert!(block.contains("Be concise."));
    }
}
