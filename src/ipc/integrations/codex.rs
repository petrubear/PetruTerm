//! Codex CLI. Hooks are merged into `~/.codex/hooks.json` (`$CODEX_HOME` honored); Codex only
//! runs them after the user trusts them once with `/hooks`. Hook commands print nothing: for
//! `UserPromptSubmit` and `SessionStart`, Codex adds stdout to the model's context. See
//! `specs/004-agent-integrations/research.md` R2.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use super::{plain_hook_command, report, session_field, session_only, str_field, AgentIntegration};
use crate::ipc::hook_files::{contains_marker, count_marked, strip_marked};
use crate::ipc::Message;
use crate::term::agents::{clean_activity, prompt_activity, relative_to};

pub struct Codex;

/// Key that holds the events in `hooks.json`. UNVERIFIED against a real file (research R2).
const HOOKS_KEY: &str = "hooks";

const EVENTS: [&str; 7] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "Stop",
    "Interrupt",
    "SessionEnd",
];

/// Events whose groups take a tool-name matcher.
const TOOL_EVENTS: [&str; 2] = ["PreToolUse", "PermissionRequest"];

/// First path named by an `apply_patch` body (`*** Add|Update|Delete File: <path>`).
fn patch_path(patch: &str) -> Option<&str> {
    patch.lines().find_map(|l| {
        let l = l.trim();
        ["*** Add File: ", "*** Update File: ", "*** Delete File: "]
            .iter()
            .find_map(|p| l.strip_prefix(p))
            .map(str::trim)
            .filter(|p| !p.is_empty())
    })
}

/// A shell command given as a string or an argv array.
fn command_text(input: &Value) -> Option<String> {
    match input.get("command")? {
        Value::String(s) => Some(s.clone()),
        Value::Array(a) => {
            let parts: Vec<&str> = a.iter().filter_map(Value::as_str).collect();
            (!parts.is_empty()).then(|| parts.join(" "))
        }
        _ => None,
    }
}

fn tool_activity(payload: &Value) -> Option<String> {
    let tool = str_field(payload, "tool_name")?;
    let input = payload.get("tool_input").unwrap_or(&Value::Null);
    let cwd = str_field(payload, "cwd");
    let target = if tool == "apply_patch" {
        str_field(input, "patch")
            .and_then(patch_path)
            .map(|p| relative_to(p, cwd).to_string())
    } else {
        command_text(input)
    };
    let text = match target {
        Some(t) if !t.trim().is_empty() => format!("{tool}: {t}"),
        _ => tool.to_string(),
    };
    clean_activity(&text)
}

fn handler(event: &str) -> Value {
    json!({ "type": "command", "command": plain_hook_command("codex", event), "timeout": 5 })
}

impl AgentIntegration for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn events(&self) -> &'static [&'static str] {
        &EVENTS
    }

    fn on_hook(&self, event: &str, payload: &Value, pane: u64) -> Option<Message> {
        let session = session_field(payload, "session_id");
        let (state, activity, body) = match event {
            "UserPromptSubmit" => (
                "working",
                str_field(payload, "prompt").and_then(prompt_activity),
                None,
            ),
            "PreToolUse" => ("working", tool_activity(payload), None),
            "PermissionRequest" => {
                let described = payload
                    .get("tool_input")
                    .and_then(|i| str_field(i, "description"))
                    .map(str::to_string);
                let tool = str_field(payload, "tool_name").unwrap_or("a tool");
                (
                    "needs-input",
                    None,
                    Some(described.unwrap_or_else(|| format!("Permission: {tool}"))),
                )
            }
            "Stop" => ("done", None, None),
            "Interrupt" | "SessionEnd" => ("idle", None, None),
            "SessionStart" => return session_only(pane, session),
            _ => return None,
        };
        Some(report(pane, "Codex", state, activity, body, session))
    }

    fn binaries(&self) -> &'static [&'static str] {
        &["codex"]
    }

    fn hook_file(&self, home: &Path) -> Option<PathBuf> {
        let base = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"));
        Some(base.join("hooks.json"))
    }

    fn install(&self, doc: &mut Value) -> Result<usize, String> {
        let events = doc
            .as_object_mut()
            .ok_or("hooks.json is not a JSON object")?
            .entry(HOOKS_KEY)
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("`hooks` in hooks.json is not an object")?;
        let mut added = 0;
        for event in EVENTS {
            let list = events
                .entry(event)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or_else(|| format!("`hooks.{event}` in hooks.json is not an array"))?;
            if list.iter().any(contains_marker) {
                continue;
            }
            let mut group = Map::new();
            if TOOL_EVENTS.contains(&event) {
                group.insert("matcher".into(), json!("*"));
            }
            group.insert("hooks".into(), json!([handler(event)]));
            list.push(Value::Object(group));
            added += 1;
        }
        Ok(added)
    }

    fn remove(&self, doc: &mut Value) -> usize {
        let Some(obj) = doc.as_object_mut() else {
            return 0;
        };
        let Some(events) = obj.get_mut(HOOKS_KEY).and_then(Value::as_object_mut) else {
            return 0;
        };
        let removed = strip_marked(events);
        if events.is_empty() {
            obj.remove(HOOKS_KEY);
        }
        removed
    }

    fn installed(&self, doc: &Value) -> usize {
        doc.get(HOOKS_KEY)
            .and_then(Value::as_object)
            .map_or(0, count_marked)
    }

    fn setup_note(&self) -> Option<&'static str> {
        Some("trust required: run /hooks in Codex once")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_case;
    use crate::ipc::integrations::contract::check_integration;

    #[test]
    fn generic_contract() {
        check_integration(&Codex);
        for e in EVENTS {
            assert_eq!(Codex.hook_stdout(e), None, "Codex hooks print nothing");
        }
    }

    #[test]
    fn fixtures() {
        let i = &Codex;
        contract_case!(i, "codex", "SessionStart", None, None);
        contract_case!(
            i,
            "codex",
            "UserPromptSubmit",
            Some("working"),
            Some("Prompt: add a test")
        );
        contract_case!(
            i,
            "codex",
            "PreToolUse_Bash",
            Some("working"),
            Some("Bash: cargo test")
        );
        contract_case!(
            i,
            "codex",
            "PreToolUse_apply_patch",
            Some("working"),
            Some("apply_patch: src/parser.rs")
        );
        contract_case!(
            i,
            "codex",
            "PreToolUse_other",
            Some("working"),
            Some("mcp__docs__search")
        );
        contract_case!(
            i,
            "codex",
            "PreToolUse_array_command",
            Some("working"),
            Some("shell: bash -lc ls -la")
        );
        contract_case!(i, "codex", "PermissionRequest", Some("needs-input"), None);
        contract_case!(
            i,
            "codex",
            "PermissionRequest_no_description",
            Some("needs-input"),
            None
        );
        contract_case!(i, "codex", "Stop", Some("done"), None);
        contract_case!(i, "codex", "Interrupt", Some("idle"), None);
        contract_case!(i, "codex", "SessionEnd", Some("idle"), None);
    }

    #[test]
    fn permission_request_carries_its_description_or_a_fallback() {
        use crate::ipc::integrations::contract::load;
        let (e, p) = load("codex", "PermissionRequest");
        assert_eq!(
            Codex.on_hook(&e, &p, 1).unwrap().body.as_deref(),
            Some("Delete the build directory")
        );
        let (e, p) = load("codex", "PermissionRequest_no_description");
        assert_eq!(
            Codex.on_hook(&e, &p, 1).unwrap().body.as_deref(),
            Some("Permission: apply_patch")
        );
    }

    #[test]
    fn patch_paths() {
        assert_eq!(
            patch_path("*** Begin Patch\n*** Delete File: a/b.rs\n"),
            Some("a/b.rs")
        );
        assert_eq!(patch_path("no headers"), None);
        assert_eq!(patch_path("*** Update File:   \n"), None);
    }

    #[test]
    fn install_merges_idempotently_and_keeps_user_hooks() {
        let user = json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}]}, "x": 1});
        let mut doc = user.clone();
        assert_eq!(Codex.install(&mut doc).unwrap(), 7);
        assert_eq!(Codex.install(&mut doc).unwrap(), 0);
        assert_eq!(Codex.installed(&doc), 7);
        let pre = &doc["hooks"]["PreToolUse"][0];
        assert_eq!(pre["matcher"], "*");
        assert_eq!(pre["hooks"][0]["timeout"], 5);
        assert!(doc["hooks"]["Stop"][1].get("matcher").is_none());
        assert_eq!(doc["hooks"]["Stop"][0], user["hooks"]["Stop"][0]);
        assert_eq!(Codex.remove(&mut doc), 7);
        assert_eq!(doc, user);
    }

    #[test]
    fn removing_from_a_file_that_was_only_ours_leaves_it_empty() {
        let mut doc = json!({});
        Codex.install(&mut doc).unwrap();
        Codex.remove(&mut doc);
        assert_eq!(doc, json!({}));
        assert!(Codex.is_empty(&doc));
    }

    #[test]
    fn rejects_unexpected_shapes() {
        assert!(Codex.install(&mut json!([])).is_err());
        assert!(Codex.install(&mut json!({"hooks": []})).is_err());
        assert!(Codex.install(&mut json!({"hooks": {"Stop": {}}})).is_err());
    }
}
