// M5d: the shared, engine-agnostic prompt-context builder -- skill-match
// instructions, steering-file rules, shell context, and attached-file
// contents, built once and consumed two ways: the direct-provider path
// appends `.text` to its system message; the ACP path prepends it to the
// user's own prompt text, since ACP has no system-message concept (see
// the M5d spec's Section 3). Ported verbatim from the wgpu build's own
// inline logic in `submit_ai_query` (`src/app/ui/mod.rs`, the block
// building `system_text` from steering/skill/shell-context/attached
// files) -- this is a pure extraction, not new logic.

use std::path::PathBuf;

use crate::llm::shell_context::ShellContext;
use crate::llm::skills::SkillManager;
use crate::llm::steering::SteeringManager;

/// Cap on a single attached file's injected content, and on the combined
/// total across all attached files (TD-030, ported verbatim).
#[allow(dead_code)]
const MAX_FILE_BYTES: usize = 512 * 1024;
#[allow(dead_code)]
const MAX_TOTAL_BYTES: usize = 1024 * 1024;

/// Extra context text to inject alongside a user's query. Empty `text`
/// means nothing applied -- callers skip appending/prepending in that
/// case rather than adding a stray blank block.
#[allow(dead_code)]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PromptAddendum {
    pub text: String,
    /// The skill that ended up active this turn, if any -- callers write
    /// this back into `ChatPanel::matched_skill`.
    pub matched_skill: Option<String>,
}

#[allow(dead_code)]
pub fn build_prompt_addendum(
    skill_manager: &SkillManager,
    steering_manager: &SteeringManager,
    active_skill_name: Option<&str>,
    user_content: &str,
    attached_files: &[PathBuf],
) -> PromptAddendum {
    let mut text = String::new();
    let mut matched_skill = None;

    // Steering files: global/project Markdown rules always active.
    if let Some(block) = steering_manager.context_block() {
        text.push_str(&format!("\n\n{block}"));
    }

    // Skill injection: match by query, or keep the conversation's active skill.
    let skill_match = if let Some(skill) = skill_manager.match_query(user_content) {
        let body = skill_manager.read_body(skill).ok();
        body.map(|b| (skill.name.clone(), b))
    } else if let Some(name) = active_skill_name {
        let found = skill_manager
            .skills()
            .iter()
            .find(|s| s.name == name)
            .cloned();
        found.and_then(|s| {
            skill_manager
                .read_body(&s)
                .ok()
                .map(|b| (name.to_string(), b))
        })
    } else {
        None
    };
    if let Some((skill_name, skill_body)) = skill_match {
        text.push_str(&format!(
            "\n\nThe following expert skill has been activated. \
             You MUST follow its instructions precisely. \
             All files referenced in the instructions (templates, guides, scripts) \
             are already included verbatim below — do NOT use file tools to read \
             them from disk, their content is already here:\n\n{skill_body}"
        ));
        matched_skill = Some(skill_name);
    }

    if let Some(ctx) = ShellContext::load() {
        text.push_str(&format!(
            "\n\nShell context:\n{}",
            ctx.format_for_system_message()
        ));
    }

    let mut total_bytes = 0usize;
    for path in attached_files {
        if total_bytes >= MAX_TOTAL_BYTES {
            break;
        }
        if let Ok(bytes) = std::fs::read(path) {
            let cap = bytes
                .len()
                .min(MAX_FILE_BYTES)
                .min(MAX_TOTAL_BYTES - total_bytes);
            let content = String::from_utf8_lossy(&bytes[..cap]);
            let name = path.display();
            text.push_str(&format!("\n\n--- File: {name} ---\n{content}"));
            if cap < bytes.len() {
                text.push_str("\n[... truncated — file exceeds size limit ...]");
            }
            total_bytes += cap;
        }
    }

    PromptAddendum {
        text,
        matched_skill,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_skill(dir: &std::path::Path, name: &str, description: &str, body: &str) {
        let skill_dir = dir.join(".petruterm/skills").join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n{body}"),
        )
        .unwrap();
    }

    fn write_steering(dir: &std::path::Path, filename: &str, content: &str) {
        let steering_dir = dir.join(".petruterm/steering");
        std::fs::create_dir_all(&steering_dir).unwrap();
        std::fs::write(steering_dir.join(filename), content).unwrap();
    }

    #[test]
    fn empty_managers_produce_no_skill_or_steering_text() {
        let skills = SkillManager::new();
        let steering = SteeringManager::new();
        let result = build_prompt_addendum(&skills, &steering, None, "hello", &[]);
        assert!(result.matched_skill.is_none());
        assert!(!result.text.contains("expert skill"));
        assert!(!result.text.contains("steering instructions"));
    }

    #[test]
    fn matching_skill_gets_injected_and_recorded() {
        let dir = TempDir::new().unwrap();
        write_skill(
            dir.path(),
            "git-helper",
            "Git branch expert",
            "Use `git switch`.",
        );
        let mut skills = SkillManager::new();
        skills.load(dir.path(), true);
        let steering = SteeringManager::new();

        let result =
            build_prompt_addendum(&skills, &steering, None, "skill git-helper please", &[]);
        assert_eq!(result.matched_skill, Some("git-helper".to_string()));
        assert!(result.text.contains("expert skill has been activated"));
        assert!(result.text.contains("Use `git switch`."));
    }

    #[test]
    fn active_skill_continues_when_no_new_match() {
        let dir = TempDir::new().unwrap();
        write_skill(
            dir.path(),
            "git-helper",
            "Git branch expert",
            "Use `git switch`.",
        );
        let mut skills = SkillManager::new();
        skills.load(dir.path(), true);
        let steering = SteeringManager::new();

        let result =
            build_prompt_addendum(&skills, &steering, Some("git-helper"), "what next?", &[]);
        assert_eq!(result.matched_skill, Some("git-helper".to_string()));
        assert!(result.text.contains("Use `git switch`."));
    }

    #[test]
    fn no_match_and_no_active_skill_leaves_matched_skill_none() {
        let dir = TempDir::new().unwrap();
        write_skill(
            dir.path(),
            "git-helper",
            "Git branch expert",
            "Use `git switch`.",
        );
        let mut skills = SkillManager::new();
        skills.load(dir.path(), true);
        let steering = SteeringManager::new();

        let result = build_prompt_addendum(&skills, &steering, None, "totally unrelated", &[]);
        assert!(result.matched_skill.is_none());
        assert!(!result.text.contains("expert skill"));
    }

    #[test]
    fn steering_block_gets_appended() {
        let dir = TempDir::new().unwrap();
        write_steering(dir.path(), "rules.md", "Be concise.");
        let skills = SkillManager::new();
        let mut steering = SteeringManager::new();
        steering.load(dir.path(), true);

        let result = build_prompt_addendum(&skills, &steering, None, "hi", &[]);
        assert!(result.text.contains("steering instructions"));
        assert!(result.text.contains("Be concise."));
    }

    #[test]
    fn attached_file_content_is_injected_with_header() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("notes.txt");
        std::fs::write(&file_path, "important notes").unwrap();
        let skills = SkillManager::new();
        let steering = SteeringManager::new();

        let result = build_prompt_addendum(&skills, &steering, None, "hi", &[file_path]);
        assert!(result.text.contains("--- File:"));
        assert!(result.text.contains("important notes"));
    }

    #[test]
    fn attached_file_over_cap_gets_truncated() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("big.txt");
        std::fs::write(&file_path, vec![b'x'; MAX_FILE_BYTES + 100]).unwrap();
        let skills = SkillManager::new();
        let steering = SteeringManager::new();

        let result = build_prompt_addendum(&skills, &steering, None, "hi", &[file_path]);
        assert!(result.text.contains("[... truncated"));
    }
}
