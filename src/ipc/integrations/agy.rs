//! Antigravity CLI (`agy`). Hooks are merged into `~/.gemini/config/hooks.json` under the hook
//! name `petruterm`. Unlike the other agents, Antigravity requires every hook to answer with
//! JSON on stdout (`PreToolUse` needs a `decision`), so each command prints the agent's
//! default reply even outside PetruTerm. See `specs/004-agent-integrations/research.md` R3.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use super::{report, session_field, session_only, str_field, AgentIntegration};
use crate::ipc::hook_files::{contains_marker, count_marked, strip_marked};
use crate::ipc::Message;
use crate::term::agents::{clean_activity, prompt_activity, relative_to};

pub struct Agy;

/// Name of our entry in `hooks.json`.
const HOOK_NAME: &str = "petruterm";

const EVENTS: [&str; 5] = [
    "SessionStart",
    "PreInvocation",
    "PreToolUse",
    "PostToolUse",
    "Stop",
];

/// Events registered as `{matcher, hooks: [..]}` groups (tool events); the rest are flat.
const TOOL_EVENTS: [&str; 2] = ["PreToolUse", "PostToolUse"];

const ALLOW: &str = r#"{"decision":"allow"}"#;
const EMPTY: &str = "{}";

/// (label, argument key, whether the argument is a path) for a known tool.
fn tool_info(name: &str) -> Option<(&'static str, &'static str, bool)> {
    Some(match name {
        "run_command" => ("Shell", "CommandLine", false),
        "view_file" => ("Read", "AbsolutePath", true),
        "write_to_file" | "replace_file_content" | "multi_replace_file_content" => {
            ("Edit", "TargetFile", true)
        }
        "list_dir" => ("List", "DirectoryPath", true),
        "find_by_name" => ("Find", "Pattern", false),
        "grep_search" => ("Grep", "Query", false),
        "search_web" => ("Search", "query", false),
        "read_url_content" => ("Fetch", "Url", false),
        _ => return None,
    })
}

fn tool_activity(payload: &Value) -> Option<String> {
    let call = payload.get("toolCall")?;
    let name = str_field(call, "name")?;
    let workspace = payload
        .get("workspacePaths")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str);
    let Some((label, key, is_path)) = tool_info(name) else {
        return clean_activity(name);
    };
    let text = match call.get("args").and_then(|a| str_field(a, key)) {
        Some(t) if !t.trim().is_empty() => {
            let t = if is_path {
                relative_to(t, workspace)
            } else {
                t
            };
            format!("{label}: {t}")
        }
        _ => label.to_string(),
    };
    clean_activity(&text)
}

/// The command each hook entry runs: our binary inside PetruTerm, the agent's default reply
/// everywhere else.
fn command(event: &str) -> String {
    let default = if event == "PreToolUse" { ALLOW } else { EMPTY };
    format!(
        r#"if [ -n "$PETRUTERM_BIN" ]; then "$PETRUTERM_BIN" hook agy {event}; else echo '{default}'; fi"#
    )
}

fn handler(event: &str) -> Value {
    json!({ "type": "command", "command": command(event), "timeout": 5 })
}

impl AgentIntegration for Agy {
    fn id(&self) -> &'static str {
        "agy"
    }

    fn events(&self) -> &'static [&'static str] {
        &EVENTS
    }

    fn on_hook(&self, event: &str, payload: &Value, pane: u64) -> Option<Message> {
        let session = session_field(payload, "conversationId");
        let (state, activity) = match event {
            "PreInvocation" => (
                "working",
                str_field(payload, "lastUserInput").and_then(prompt_activity),
            ),
            "PreToolUse" => ("working", tool_activity(payload)),
            // `fullyIdle: false` means background work continues: no change.
            "Stop" if payload.get("fullyIdle").and_then(Value::as_bool) == Some(true) => {
                ("done", None)
            }
            "SessionStart" => return session_only(pane, session),
            _ => return None,
        };
        Some(report(pane, "Antigravity", state, activity, None, session))
    }

    fn hook_stdout(&self, event: &str) -> Option<&'static str> {
        match event {
            "PreToolUse" => Some(ALLOW),
            "PostToolUse" | "PreInvocation" | "PostInvocation" | "Stop" | "SessionStart" => {
                Some(EMPTY)
            }
            _ => None,
        }
    }

    fn binaries(&self) -> &'static [&'static str] {
        &["agy", "antigravity"]
    }

    fn hook_file(&self, home: &Path) -> Option<PathBuf> {
        Some(home.join(".gemini/config/hooks.json"))
    }

    fn install(&self, doc: &mut Value) -> Result<usize, String> {
        let events = doc
            .as_object_mut()
            .ok_or("hooks.json is not a JSON object")?
            .entry(HOOK_NAME)
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("the `petruterm` entry in hooks.json is not an object")?;
        let mut added = 0;
        for event in EVENTS {
            let list = events
                .entry(event)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or_else(|| format!("`{HOOK_NAME}.{event}` in hooks.json is not an array"))?;
            if list.iter().any(contains_marker) {
                continue;
            }
            if TOOL_EVENTS.contains(&event) {
                let mut group = Map::new();
                group.insert("matcher".into(), json!("*"));
                group.insert("hooks".into(), json!([handler(event)]));
                list.push(Value::Object(group));
            } else {
                list.push(handler(event));
            }
            added += 1;
        }
        Ok(added)
    }

    fn remove(&self, doc: &mut Value) -> usize {
        let Some(obj) = doc.as_object_mut() else {
            return 0;
        };
        let Some(events) = obj.get_mut(HOOK_NAME).and_then(Value::as_object_mut) else {
            return 0;
        };
        let removed = strip_marked(events);
        if events.is_empty() {
            obj.remove(HOOK_NAME);
        }
        removed
    }

    fn installed(&self, doc: &Value) -> usize {
        doc.get(HOOK_NAME)
            .and_then(Value::as_object)
            .map_or(0, count_marked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_case;
    use crate::ipc::integrations::contract::{check_integration, load};

    #[test]
    fn generic_contract() {
        check_integration(&Agy);
    }

    #[test]
    fn fixtures() {
        let i = &Agy;
        contract_case!(i, "agy", "SessionStart", None, None);
        contract_case!(
            i,
            "agy",
            "PreInvocation",
            Some("working"),
            Some("Prompt: explain the build")
        );
        contract_case!(
            i,
            "agy",
            "PreToolUse_run_command",
            Some("working"),
            Some("Shell: cargo build")
        );
        contract_case!(
            i,
            "agy",
            "PreToolUse_view_file",
            Some("working"),
            Some("Read: src/lib.rs")
        );
        contract_case!(
            i,
            "agy",
            "PreToolUse_write_to_file",
            Some("working"),
            Some("Edit: notes.md")
        );
        contract_case!(
            i,
            "agy",
            "PreToolUse_unknown",
            Some("working"),
            Some("manage_task")
        );
        contract_case!(i, "agy", "PostToolUse", None, None);
        contract_case!(i, "agy", "PostInvocation", None, None);
        contract_case!(i, "agy", "Stop_idle", Some("done"), None);
        contract_case!(i, "agy", "Stop_not_idle", None, None);
    }

    #[test]
    fn session_id_comes_from_conversation_id() {
        let (e, p) = load("agy", "Stop_idle");
        assert_eq!(
            Agy.on_hook(&e, &p, 1).unwrap().session.as_deref(),
            Some("conv-agy-1")
        );
        let (e, p) = load("agy", "SessionStart");
        assert_eq!(
            Agy.on_hook(&e, &p, 1).unwrap().session.as_deref(),
            Some("conv-agy-1")
        );
    }

    #[test]
    fn stdout_is_the_required_json_for_every_event() {
        assert_eq!(
            Agy.hook_stdout("PreToolUse"),
            Some(r#"{"decision":"allow"}"#)
        );
        for e in [
            "PostToolUse",
            "PreInvocation",
            "PostInvocation",
            "Stop",
            "SessionStart",
        ] {
            assert_eq!(Agy.hook_stdout(e), Some("{}"), "{e}");
        }
        assert_eq!(Agy.hook_stdout("Other"), None);
    }

    #[test]
    fn install_merges_under_our_hook_name_and_removes_cleanly() {
        let user = json!({"lint": {"PostToolUse": [{"matcher": "run_command", "hooks": [{"type": "command", "command": "./lint.sh"}]}]}});
        let mut doc = user.clone();
        assert_eq!(Agy.install(&mut doc).unwrap(), 5);
        assert_eq!(Agy.install(&mut doc).unwrap(), 0);
        assert_eq!(Agy.installed(&doc), 5);
        assert_eq!(doc["lint"], user["lint"]);
        let pre = &doc["petruterm"]["PreToolUse"][0];
        assert_eq!(pre["matcher"], "*");
        assert_eq!(pre["hooks"][0]["timeout"], 5);
        assert_eq!(doc["petruterm"]["Stop"][0]["type"], "command");
        assert_eq!(Agy.remove(&mut doc), 5);
        assert_eq!(doc, user);
    }

    #[test]
    fn commands_print_the_default_reply_when_run_outside_petruterm() {
        for (event, want) in [
            ("PreToolUse", r#"{"decision":"allow"}"#),
            ("PostToolUse", "{}"),
            ("Stop", "{}"),
            ("PreInvocation", "{}"),
            ("SessionStart", "{}"),
        ] {
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(command(event))
                .env_remove("PETRUTERM_BIN")
                .output()
                .unwrap();
            assert!(out.status.success(), "{event}");
            assert_eq!(
                String::from_utf8(out.stdout).unwrap().trim(),
                want,
                "{event}"
            );
        }
    }

    #[test]
    fn rejects_unexpected_shapes() {
        assert!(Agy.install(&mut json!([])).is_err());
        assert!(Agy.install(&mut json!({"petruterm": 3})).is_err());
    }
}
