//! The "what is the agent doing" line of an Agents row: building it from a hook payload,
//! cleaning it, and the config-selected detail level. Pure logic, shared by both binaries.
//! See `specs/003-agent-activity-cards/`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Longest activity text, in characters, including a trailing `...`.
pub const MAX_ACTIVITY_CHARS: usize = 80;

/// How much of the activity the Agents row shows (`agents.activity` in the config).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityDetail {
    /// Tool and target, e.g. `Bash: cargo test`.
    #[default]
    Full,
    /// Only the label before the first `": "`, e.g. `Bash`.
    Tool,
    /// No activity line.
    Off,
}

impl ActivityDetail {
    /// Parse the config string; anything unknown is `None` (the caller keeps its default).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "full" => Some(Self::Full),
            "tool" => Some(Self::Tool),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
}

/// One line, no control characters, whitespace runs collapsed, at most
/// [`MAX_ACTIVITY_CHARS`] characters (cut with `...`). `None` when nothing is left.
pub fn clean_activity(text: &str) -> Option<String> {
    let mut out = String::new();
    for word in text
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|w| !w.is_empty())
    {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        return None;
    }
    if out.chars().count() > MAX_ACTIVITY_CHARS {
        let head: String = out.chars().take(MAX_ACTIVITY_CHARS - 3).collect();
        out = format!("{}...", head.trim_end());
    }
    Some(out)
}

/// What to show for `text` at the given detail level.
pub fn apply_detail(detail: ActivityDetail, text: &str) -> Option<String> {
    match detail {
        ActivityDetail::Full => Some(text.to_string()),
        ActivityDetail::Tool => Some(
            text.split_once(": ")
                .map_or(text, |(label, _)| label)
                .to_string(),
        ),
        ActivityDetail::Off => None,
    }
}

/// `path` relative to `cwd` when it lies under it (on a path boundary), else unchanged.
pub fn relative_to<'a>(path: &'a str, cwd: Option<&str>) -> &'a str {
    let Some(cwd) = cwd
        .map(|c| c.trim_end_matches('/'))
        .filter(|c| !c.is_empty())
    else {
        return path;
    };
    match path.strip_prefix(cwd) {
        Some(rest) if rest.starts_with('/') && rest.len() > 1 => &rest[1..],
        _ => path,
    }
}

/// `Tool: target` for a `PreToolUse` payload (`tool_name`, `tool_input`, `cwd`); the tool name
/// alone when there is no recognized target. `None` for an empty tool name.
pub fn tool_activity(tool: &str, input: &Value, cwd: Option<&str>) -> Option<String> {
    let field = |k: &str| input.get(k).and_then(Value::as_str);
    let target = match tool {
        "Bash" => field("command"),
        "Edit" | "Write" | "Read" | "MultiEdit" => field("file_path").map(|p| relative_to(p, cwd)),
        "NotebookEdit" => field("notebook_path")
            .or_else(|| field("file_path"))
            .map(|p| relative_to(p, cwd)),
        "Grep" | "Glob" => field("pattern"),
        "WebFetch" => field("url"),
        "WebSearch" => field("query"),
        "Agent" | "Task" => field("description"),
        _ => None,
    };
    match target.filter(|t| !t.trim().is_empty()) {
        Some(t) => clean_activity(&format!("{tool}: {t}")),
        None => clean_activity(tool),
    }
}

/// `Prompt: <first line>` for a `UserPromptSubmit` payload.
pub fn prompt_activity(prompt: &str) -> Option<String> {
    let line = prompt.lines().map(str::trim).find(|l| !l.is_empty())?;
    clean_activity(&format!("Prompt: {line}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn clean_collapses_whitespace_and_controls() {
        assert_eq!(clean_activity("a\n b\t\tc  d").as_deref(), Some("a b c d"));
        let c = clean_activity("x\x1b[31my\x07z").unwrap();
        assert!(!c.chars().any(char::is_control));
        assert_eq!(clean_activity(""), None);
        assert_eq!(clean_activity(" \n\t \x01 "), None);
    }

    #[test]
    fn clean_caps_at_80_characters_with_dots() {
        let exact = "a".repeat(80);
        assert_eq!(clean_activity(&exact).as_deref(), Some(exact.as_str()));
        let long = clean_activity(&"a".repeat(81)).unwrap();
        assert_eq!(long.chars().count(), 80);
        assert!(long.ends_with("..."));
        let huge = clean_activity(&"word ".repeat(2000)).unwrap();
        assert!(huge.chars().count() <= 80 && huge.ends_with("..."));
    }

    #[test]
    fn clean_never_splits_a_character() {
        for s in ["é", "日本語", "😀"] {
            let out = clean_activity(&s.repeat(100)).unwrap();
            assert_eq!(out.chars().count(), 80);
            assert!(out.ends_with("..."));
        }
    }

    #[test]
    fn detail_levels() {
        let t = "Bash: cargo test";
        assert_eq!(apply_detail(ActivityDetail::Full, t).as_deref(), Some(t));
        assert_eq!(
            apply_detail(ActivityDetail::Tool, t).as_deref(),
            Some("Bash")
        );
        assert_eq!(
            apply_detail(ActivityDetail::Tool, "Thinking").as_deref(),
            Some("Thinking")
        );
        assert_eq!(
            apply_detail(ActivityDetail::Tool, "Edit: a: b").as_deref(),
            Some("Edit")
        );
        assert_eq!(apply_detail(ActivityDetail::Off, t), None);
    }

    #[test]
    fn detail_parse() {
        assert_eq!(ActivityDetail::parse("tool"), Some(ActivityDetail::Tool));
        assert_eq!(ActivityDetail::parse("nope"), None);
        assert_eq!(ActivityDetail::default(), ActivityDetail::Full);
    }

    #[test]
    fn tool_targets() {
        let a = |tool: &str, input: Value| tool_activity(tool, &input, Some("/p/x")).unwrap();
        assert_eq!(
            a("Bash", json!({"command": "cargo test\n--lib"})),
            "Bash: cargo test --lib"
        );
        assert_eq!(
            a("Edit", json!({"file_path": "/p/x/src/a.rs"})),
            "Edit: src/a.rs"
        );
        assert_eq!(
            a("Write", json!({"file_path": "/other/b.rs"})),
            "Write: /other/b.rs"
        );
        assert_eq!(
            a("Read", json!({"file_path": "/p/xy/c.rs"})),
            "Read: /p/xy/c.rs"
        );
        assert_eq!(
            a("Grep", json!({"pattern": "AgentState"})),
            "Grep: AgentState"
        );
        assert_eq!(a("Glob", json!({"pattern": "**/*.rs"})), "Glob: **/*.rs");
        assert_eq!(
            a("WebFetch", json!({"url": "https://a.b/c"})),
            "WebFetch: https://a.b/c"
        );
        assert_eq!(
            a("WebSearch", json!({"query": "rust gpui"})),
            "WebSearch: rust gpui"
        );
        assert_eq!(
            a("Agent", json!({"description": "Find usages"})),
            "Agent: Find usages"
        );
        assert_eq!(
            a("NotebookEdit", json!({"notebook_path": "/p/x/n.ipynb"})),
            "NotebookEdit: n.ipynb"
        );
    }

    #[test]
    fn tool_without_target_shows_the_name() {
        let a = |tool: &str, input: Value| tool_activity(tool, &input, None).unwrap();
        assert_eq!(a("TodoWrite", json!({"todos": []})), "TodoWrite");
        assert_eq!(a("mcp__x__y", json!({"q": 1})), "mcp__x__y");
        assert_eq!(a("Bash", json!({})), "Bash");
        assert_eq!(a("Bash", json!({"command": 5})), "Bash");
        assert_eq!(a("Bash", json!({"command": "  "})), "Bash");
        assert_eq!(a("Read", Value::Null), "Read");
        assert_eq!(tool_activity("", &Value::Null, None), None);
    }

    #[test]
    fn prompt_uses_the_first_non_empty_line() {
        assert_eq!(
            prompt_activity("\n  fix the bug\nmore").as_deref(),
            Some("Prompt: fix the bug")
        );
        assert_eq!(prompt_activity("  \n "), None);
        assert!(prompt_activity(&"x".repeat(500)).unwrap().chars().count() <= 80);
    }
}
