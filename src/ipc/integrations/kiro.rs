//! Kiro CLI (V3 mode). Hooks live in a global file we own, `~/.kiro/hooks/petruterm.json`
//! (`$KIRO_HOME` honored). Hook commands print nothing: Kiro adds a command's stdout to the
//! model's context and blocks a tool call when a `PreToolUse` hook fails. See
//! `specs/004-agent-integrations/research.md` R1.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::{plain_hook_command, report, session_field, session_only, str_field, AgentIntegration};
use crate::ipc::hook_files::contains_marker;
use crate::ipc::Message;
use crate::term::agents::{clean_activity, prompt_activity, relative_to};

pub struct Kiro;

const EVENTS: [&str; 5] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "Stop",
    "SessionEnd",
];

/// Display label of a Kiro tool (canonical names and their aliases).
fn label(tool: &str) -> &str {
    match tool {
        "execute_bash" | "shell" => "Shell",
        "fs_read" | "read" => "Read",
        "fs_write" | "write" => "Write",
        "use_aws" | "aws" => "AWS",
        other => other,
    }
}

/// The most informative argument of a tool call, tried in a fixed order. File arguments are
/// shown relative to `cwd`.
fn target(input: &Value, cwd: Option<&str>) -> Option<String> {
    let s = |k: &str| str_field(input, k);
    // `command` is the shell command, except on file tools where it is an edit verb.
    let edit_verb = |c: &str| {
        matches!(
            c,
            "create" | "str_replace" | "insert" | "append" | "view" | "delete" | "undo_edit"
        )
    };
    if let Some(c) = s("command").filter(|c| !(input.get("path").is_some() && edit_verb(c))) {
        return Some(c.to_string());
    }
    let path = s("path")
        .or_else(|| s("file_path"))
        .or_else(|| s("file"))
        .or_else(|| {
            input
                .get("paths")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(Value::as_str)
        })
        .or_else(|| {
            input
                .get("operations")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(|op| str_field(op, "path"))
        })?;
    Some(relative_to(path, cwd).to_string())
}

fn tool_activity(payload: &Value) -> Option<String> {
    let tool = str_field(payload, "tool_name")?;
    let cwd = str_field(payload, "cwd");
    // MCP tools (`@server/tool`) and unknown tools: the name alone.
    let text = match payload.get("tool_input").and_then(|i| target(i, cwd)) {
        Some(t) if !tool.starts_with('@') => format!("{}: {t}", label(tool)),
        _ => label(tool).to_string(),
    };
    clean_activity(&text)
}

impl AgentIntegration for Kiro {
    fn id(&self) -> &'static str {
        "kiro"
    }

    fn events(&self) -> &'static [&'static str] {
        &EVENTS
    }

    fn on_hook(&self, event: &str, payload: &Value, pane: u64) -> Option<Message> {
        let session = session_field(payload, "session_id");
        let (state, activity) = match event {
            "UserPromptSubmit" => (
                "working",
                str_field(payload, "prompt").and_then(prompt_activity),
            ),
            "PreToolUse" => ("working", tool_activity(payload)),
            "Stop" => ("done", None),
            "SessionEnd" => ("idle", None),
            "SessionStart" => return session_only(pane, session),
            _ => return None,
        };
        Some(report(pane, "Kiro", state, activity, None, session))
    }

    fn binaries(&self) -> &'static [&'static str] {
        &["kiro-cli", "kiro"]
    }

    fn hook_file(&self, home: &Path) -> Option<PathBuf> {
        let base = std::env::var_os("KIRO_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".kiro"));
        Some(base.join("hooks/petruterm.json"))
    }

    fn install(&self, doc: &mut Value) -> Result<usize, String> {
        let obj = doc
            .as_object_mut()
            .ok_or("hook file is not a JSON object")?;
        obj.entry("version").or_insert_with(|| json!("v1"));
        let hooks = obj
            .entry("hooks")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("`hooks` in the Kiro hook file is not an array")?;
        let mut added = 0;
        for event in EVENTS {
            let ours = |h: &Value| contains_marker(h) && str_field(h, "trigger") == Some(event);
            if hooks.iter().any(ours) {
                continue;
            }
            hooks.push(json!({
                "name": format!("petruterm-{event}"),
                "trigger": event,
                "action": { "type": "command", "command": plain_hook_command("kiro", event) },
                "timeout": 5
            }));
            added += 1;
        }
        Ok(added)
    }

    fn remove(&self, doc: &mut Value) -> usize {
        let Some(hooks) = doc.get_mut("hooks").and_then(Value::as_array_mut) else {
            return 0;
        };
        let before = hooks.len();
        hooks.retain(|h| !contains_marker(h));
        before - hooks.len()
    }

    fn installed(&self, doc: &Value) -> usize {
        doc.get("hooks")
            .and_then(Value::as_array)
            .map_or(0, |h| h.iter().filter(|h| contains_marker(h)).count())
    }

    fn is_empty(&self, doc: &Value) -> bool {
        doc.get("hooks")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
    }

    fn setup_note(&self) -> Option<&'static str> {
        Some("requires Kiro CLI V3 mode (Kiro CLI 2.13 or later)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_case;
    use crate::ipc::integrations::contract::check_integration;

    #[test]
    fn generic_contract() {
        check_integration(&Kiro);
        for e in EVENTS {
            assert_eq!(Kiro.hook_stdout(e), None, "Kiro hooks print nothing");
        }
    }

    #[test]
    fn fixtures() {
        let i = &Kiro;
        contract_case!(i, "kiro", "SessionStart", None, None);
        contract_case!(
            i,
            "kiro",
            "UserPromptSubmit",
            Some("working"),
            Some("Prompt: list the files")
        );
        contract_case!(
            i,
            "kiro",
            "PreToolUse_execute_bash",
            Some("working"),
            Some("Shell: ls -la")
        );
        contract_case!(
            i,
            "kiro",
            "PreToolUse_fs_read",
            Some("working"),
            Some("Read: src/main.rs")
        );
        contract_case!(
            i,
            "kiro",
            "PreToolUse_fs_write",
            Some("working"),
            Some("Write: notes.md")
        );
        contract_case!(
            i,
            "kiro",
            "PreToolUse_mcp",
            Some("working"),
            Some("@postgres/query")
        );
        contract_case!(
            i,
            "kiro",
            "PreToolUse_no_input",
            Some("working"),
            Some("Shell")
        );
        contract_case!(i, "kiro", "Stop", Some("done"), None);
        contract_case!(i, "kiro", "SessionEnd", Some("idle"), None);
    }

    #[test]
    fn mcp_and_odd_inputs_show_the_label_only() {
        let a = |v: Value| tool_activity(&v);
        assert_eq!(
            a(json!({"tool_name": "@x/y", "tool_input": {"command": "z"}})).as_deref(),
            Some("@x/y")
        );
        assert_eq!(
            a(json!({"tool_name": "use_aws", "tool_input": 5})).as_deref(),
            Some("AWS")
        );
        assert_eq!(
            a(json!({"tool_name": "shell", "tool_input": {"command": 7}})).as_deref(),
            Some("Shell")
        );
        assert_eq!(a(json!({"tool_input": {}})), None);
    }

    #[test]
    fn session_id_is_forwarded_when_valid() {
        let (e, p) = crate::ipc::integrations::contract::load("kiro", "Stop");
        assert_eq!(
            Kiro.on_hook(&e, &p, 3).unwrap().session.as_deref(),
            Some("kiro-sess-1")
        );
        let bad = json!({"session_id": "../x"});
        assert_eq!(Kiro.on_hook("Stop", &bad, 3).unwrap().session, None);
        assert!(Kiro.on_hook("SessionStart", &bad, 3).is_none());
    }

    #[test]
    fn install_adds_every_event_once_and_remove_restores() {
        let mut doc = json!({});
        assert_eq!(Kiro.install(&mut doc).unwrap(), 5);
        assert_eq!(Kiro.install(&mut doc).unwrap(), 0);
        assert_eq!(Kiro.installed(&doc), 5);
        let first = &doc["hooks"][0];
        assert_eq!(first["action"]["type"], "command");
        assert!(first["action"]["command"]
            .as_str()
            .unwrap()
            .contains("PETRUTERM_BIN"));
        assert_eq!(first["timeout"], 5);
        assert_eq!(doc["version"], "v1");
        assert_eq!(Kiro.remove(&mut doc), 5);
        assert!(Kiro.is_empty(&doc));
    }

    #[test]
    fn user_hooks_in_the_same_file_are_kept() {
        let user = json!({"name": "mine", "trigger": "Stop", "action": {"type": "command", "command": "say done"}});
        let mut doc = json!({"version": "v1", "hooks": [user.clone()]});
        Kiro.install(&mut doc).unwrap();
        Kiro.remove(&mut doc);
        assert_eq!(doc, json!({"version": "v1", "hooks": [user]}));
        assert!(!Kiro.is_empty(&doc));
    }

    #[test]
    fn rejects_unexpected_shapes() {
        assert!(Kiro.install(&mut json!([])).is_err());
        assert!(Kiro.install(&mut json!({"hooks": {}})).is_err());
    }

    #[test]
    fn hook_file_is_under_dot_kiro() {
        if std::env::var_os("KIRO_HOME").is_none() {
            assert_eq!(
                Kiro.hook_file(Path::new("/h")),
                Some(PathBuf::from("/h/.kiro/hooks/petruterm.json"))
            );
        }
    }
}
