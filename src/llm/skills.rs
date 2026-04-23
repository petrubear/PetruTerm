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

    /// Return the skill explicitly named in `query` (e.g. "use skill git-helper ..."),
    /// or the best fuzzy match against skill descriptions, or `None`.
    pub fn match_query(&self, query: &str) -> Option<&SkillMeta> {
        // Explicit name match: "skill <name>" or "el skill <name>" anywhere in query.
        let lower = query.to_lowercase();
        for skill in &self.skills {
            let needle = skill.name.to_lowercase();
            if lower.contains(&needle) {
                return Some(skill);
            }
        }

        let mut best: Option<(i64, &SkillMeta)> = None;
        for skill in &self.skills {
            let score = self
                .matcher
                .fuzzy_match(&skill.description, query)
                .unwrap_or(0);
            if score >= SKILL_SCORE_THRESHOLD && best.is_none_or(|(b, _)| score > b) {
                best = Some((score, skill));
            }
        }
        best.map(|(_, s)| s)
    }

    /// Read the full body of a skill, including all asset files in the skill directory.
    pub fn read_body(&self, skill: &SkillMeta) -> Result<String> {
        let content = std::fs::read_to_string(&skill.path)?;
        let mut body = extract_body(&content);

        let skill_dir = skill.path.parent().unwrap_or(Path::new("."));
        let mut asset_files = collect_skill_files(skill_dir);
        asset_files.sort();

        for asset_path in asset_files {
            let rel = asset_path
                .strip_prefix(skill_dir)
                .unwrap_or(&asset_path)
                .display()
                .to_string();
            if let Ok(asset_content) = std::fs::read_to_string(&asset_path) {
                body.push_str(&format!("\n\n---\n## {rel}\n\n{asset_content}"));
            }
        }

        Ok(body)
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

/// Recursively collect all readable files in `dir`, excluding SKILL.md and hidden files.
fn collect_skill_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "SKILL.md" {
            continue;
        }
        if path.is_dir() {
            files.extend(collect_skill_files(&path));
        } else if path.is_file() {
            files.push(path);
        }
    }
    files
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
/// Handles inline values (`key: value`) and block scalars (`key: >` / `key: |`).
fn parse_frontmatter(content: &str) -> Option<(String, String)> {
    let body = content.trim_start();
    let body = body.strip_prefix("---")?;
    let end = body.find("\n---")?;
    let fm = &body[..end];

    let mut name = None;
    let mut description = None;
    let mut in_desc_block = false;
    let mut desc_lines: Vec<String> = Vec::new();

    for line in fm.lines() {
        if let Some(val) = line.strip_prefix("name:") {
            in_desc_block = false;
            name = Some(val.trim().to_string());
        } else if let Some(val) = line.strip_prefix("description:") {
            let val = val.trim();
            if val == ">" || val == "|" {
                in_desc_block = true;
                desc_lines.clear();
            } else {
                in_desc_block = false;
                description = Some(val.to_string());
            }
        } else if in_desc_block {
            if line.starts_with(' ') || line.starts_with('\t') {
                desc_lines.push(line.trim().to_string());
            } else {
                in_desc_block = false;
            }
        }
    }

    if description.is_none() && !desc_lines.is_empty() {
        description = Some(desc_lines.join(" "));
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
