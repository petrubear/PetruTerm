use anyhow::Result;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use std::path::{Path, PathBuf};

const SKILL_SCORE_THRESHOLD: i64 = 50;

#[derive(Debug, Clone)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
}

pub struct SkillManager {
    skills: Vec<SkillMeta>,
    matcher: SkimMatcherV2,
}

impl SkillManager {
    pub fn new() -> Self {
        Self {
            skills: Vec::new(),
            matcher: SkimMatcherV2::default(),
        }
    }

    /// Load global skills from `~/.config/petruterm/skills/`, then overlay
    /// project-local skills from `<cwd>/.petruterm/skills/` (project wins on name clash).
    pub fn load(&mut self, cwd: &Path) {
        self.skills.clear();

        if let Some(home) = dirs::home_dir() {
            let global = home.join(".config/petruterm/skills");
            self.scan_dir(&global);
        }

        let local = cwd.join(".petruterm/skills");
        self.scan_dir_overlay(&local);
    }

    /// Reload only the project-local skills (called on CWD change).
    pub fn reload_local(&mut self, cwd: &Path) {
        // Remove all skills that came from a project-local path under the previous cwd,
        // then re-scan the new cwd. Simplest: retain only global entries (home dir),
        // then overlay the new local dir.
        if let Some(home) = dirs::home_dir() {
            let global_prefix = home.join(".config/petruterm/skills");
            self.skills.retain(|s| s.path.starts_with(&global_prefix));
        } else {
            self.skills.clear();
        }
        let local = cwd.join(".petruterm/skills");
        self.scan_dir_overlay(&local);
    }

    /// Return the best-matching skill for `query`, or `None` if score < threshold.
    pub fn match_query(&self, query: &str) -> Option<&SkillMeta> {
        let mut best: Option<(i64, &SkillMeta)> = None;
        for skill in &self.skills {
            let score = self
                .matcher
                .fuzzy_match(&skill.description, query)
                .unwrap_or(0);
            if score >= SKILL_SCORE_THRESHOLD {
                if best.map_or(true, |(b, _)| score > b) {
                    best = Some((score, skill));
                }
            }
        }
        best.map(|(_, s)| s)
    }

    /// Read the full body of a skill (everything after the frontmatter).
    pub fn read_body(&self, skill: &SkillMeta) -> Result<String> {
        let content = std::fs::read_to_string(&skill.path)?;
        Ok(extract_body(&content))
    }

    /// All loaded skill metadata (for `/skill` listing).
    pub fn skills(&self) -> &[SkillMeta] {
        &self.skills
    }

    // ── private ───────────────────────────────────────────────────────────────

    fn scan_dir(&mut self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let skill_file = entry.path().join("SKILL.md");
            if skill_file.exists() {
                if let Some(meta) = parse_skill_file(&skill_file) {
                    self.skills.push(meta);
                }
            }
        }
    }

    /// Like `scan_dir` but replaces existing skills with the same name.
    fn scan_dir_overlay(&mut self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let skill_file = entry.path().join("SKILL.md");
            if skill_file.exists() {
                if let Some(meta) = parse_skill_file(&skill_file) {
                    if let Some(pos) = self.skills.iter().position(|s| s.name == meta.name) {
                        self.skills[pos] = meta;
                    } else {
                        self.skills.push(meta);
                    }
                }
            }
        }
    }
}

/// Parse a SKILL.md file into `SkillMeta` (name + description only — body is lazy).
fn parse_skill_file(path: &Path) -> Option<SkillMeta> {
    let content = std::fs::read_to_string(path).ok()?;
    let (name, description) = parse_frontmatter(&content)?;
    Some(SkillMeta {
        name,
        description,
        path: path.to_path_buf(),
    })
}

/// Extract `name` and `description` from YAML-style frontmatter `--- ... ---`.
fn parse_frontmatter(content: &str) -> Option<(String, String)> {
    let body = content.trim_start();
    let body = body.strip_prefix("---")?;
    let end = body.find("\n---")?;
    let fm = &body[..end];

    let mut name = None;
    let mut description = None;

    for line in fm.lines() {
        if let Some(val) = line.strip_prefix("name:") {
            name = Some(val.trim().to_string());
        } else if let Some(val) = line.strip_prefix("description:") {
            description = Some(val.trim().to_string());
        }
    }

    Some((name?, description?))
}

/// Return everything after the closing `---` of the frontmatter.
fn extract_body(content: &str) -> String {
    let body = content.trim_start();
    let Some(after_open) = body.strip_prefix("---") else {
        return content.to_string();
    };
    let Some(close_pos) = after_open.find("\n---") else {
        return content.to_string();
    };
    // skip past the closing `---\n`
    let rest = &after_open[close_pos + 4..];
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    rest.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frontmatter_basic() {
        let content = "---\nname: git-helper\ndescription: Git expert for branches\n---\nBody here";
        let (name, desc) = parse_frontmatter(content).unwrap();
        assert_eq!(name, "git-helper");
        assert_eq!(desc, "Git expert for branches");
    }

    #[test]
    fn extract_body_basic() {
        let content = "---\nname: foo\ndescription: bar\n---\nActual body\nline two";
        let body = extract_body(content);
        assert_eq!(body, "Actual body\nline two");
    }

    #[test]
    fn extract_body_no_frontmatter() {
        let content = "Just plain text";
        assert_eq!(extract_body(content), "Just plain text");
    }
}
