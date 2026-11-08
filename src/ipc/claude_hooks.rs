//! Claude Code integration: the hook definitions (shared by the `claude` shim and
//! `petruterm hooks setup claude`) and the mapping from a hook event to a [`Message`].

use super::Message;
use crate::term::agents::{prompt_activity, tool_activity};
use serde_json::{json, Map, Value};

/// Hook events we register, with an optional tool matcher.
pub const EVENTS: [(&str, Option<&str>); 7] = [
    ("SessionStart", None),
    ("UserPromptSubmit", None),
    ("PreToolUse", Some("*")),
    ("Notification", None),
    ("Stop", None),
    ("SubagentStop", None),
    ("SessionEnd", None),
];

/// Marker that identifies hooks installed by us (used to stay idempotent).
const MARKER: &str = "PETRUTERM_BIN";

/// Shell command run by the hook. Silent no-op outside PetruTerm (`PETRUTERM_BIN` unset).
pub fn hook_command(event: &str) -> String {
    format!("[ -z \"${MARKER}\" ] || \"${MARKER}\" hook claude {event}")
}

fn group(event: &str, matcher: Option<&str>) -> Value {
    let mut g = Map::new();
    if let Some(m) = matcher {
        g.insert("matcher".into(), json!(m));
    }
    g.insert(
        "hooks".into(),
        json!([{ "type": "command", "command": hook_command(event), "timeout": 5 }]),
    );
    Value::Object(g)
}

/// The `{"hooks": {...}}` settings fragment.
#[cfg_attr(not(test), allow(dead_code))] // reference definition: checked against the shim in tests/
pub fn settings_fragment() -> Value {
    let hooks: Map<String, Value> = EVENTS
        .iter()
        .map(|(event, matcher)| (event.to_string(), json!([group(event, *matcher)])))
        .collect();
    json!({ "hooks": hooks })
}

/// Add our hooks to a Claude Code `settings.json` value, keeping everything else. Events
/// that already carry one of our hooks are left alone. Returns the events that were added.
pub fn merge_into(settings: &mut Value) -> Result<Vec<&'static str>, String> {
    let root = settings
        .as_object_mut()
        .ok_or("settings.json is not a JSON object")?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("`hooks` in settings.json is not an object")?;
    let mut added = Vec::new();
    for (event, matcher) in EVENTS {
        let list = hooks
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| format!("`hooks.{event}` in settings.json is not an array"))?;
        if list.iter().any(|g| g.to_string().contains(MARKER)) {
            continue;
        }
        list.push(group(event, matcher));
        added.push(event);
    }
    Ok(added)
}

/// Map a Claude Code hook invocation to a socket message. `payload` is the JSON Claude Code
/// passes on stdin (`Null` when absent or malformed).
pub fn message_for(event: &str, payload: &Value, pane: u64) -> Option<Message> {
    let session = payload
        .get("session_id")
        .and_then(Value::as_str)
        .and_then(crate::term::agents::sanitize_session);
    let (state, body) = match event {
        // Announces the session id only; the state is left as it is.
        "SessionStart" => {
            return session.map(|session| Message {
                pane,
                session: Some(session),
                ..Message::default()
            })
        }
        "UserPromptSubmit" | "PreToolUse" => ("working", None),
        "Notification" => (
            "needs-input",
            Some(
                payload
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Claude Code needs your attention")
                    .to_string(),
            ),
        ),
        "Stop" | "SubagentStop" => ("done", None),
        "SessionEnd" => ("idle", None),
        _ => return None,
    };
    Some(Message {
        pane,
        state: Some(state.into()),
        title: Some("Claude Code".into()),
        body,
        activity: activity_for(event, payload),
        session,
        ..Message::default()
    })
}

/// What the agent is doing, from the hook payload: the prompt start on `UserPromptSubmit`, the
/// tool and its target on `PreToolUse`. Nothing for the other events.
fn activity_for(event: &str, payload: &Value) -> Option<String> {
    match event {
        "UserPromptSubmit" => payload
            .get("prompt")
            .and_then(Value::as_str)
            .and_then(prompt_activity),
        "PreToolUse" => {
            let tool = payload.get("tool_name").and_then(Value::as_str)?;
            let input = payload.get("tool_input").unwrap_or(&Value::Null);
            let cwd = payload.get("cwd").and_then(Value::as_str);
            tool_activity(tool, input, cwd)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_mapping() {
        let none = Value::Null;
        let state = |e: &str| message_for(e, &none, 1).unwrap().state.unwrap();
        assert_eq!(state("UserPromptSubmit"), "working");
        assert_eq!(state("PreToolUse"), "working");
        assert_eq!(state("Stop"), "done");
        assert_eq!(state("SubagentStop"), "done");
        assert_eq!(state("SessionEnd"), "idle");
        assert!(message_for("PostToolUse", &none, 1).is_none());
    }

    #[test]
    fn session_id_is_forwarded_and_session_start_carries_only_it() {
        let id = "3f6c0a52-91d4-4b1e-8c3a-0d2f7e5b9a11";
        let payload = json!({ "session_id": id });
        let start = message_for("SessionStart", &payload, 3).unwrap();
        assert_eq!(start.session.as_deref(), Some(id));
        assert!(start.state.is_none() && start.body.is_none());
        let work = message_for("UserPromptSubmit", &payload, 3).unwrap();
        assert_eq!(work.session.as_deref(), Some(id));
        assert!(message_for("SessionStart", &Value::Null, 3).is_none());
        let bad = json!({ "session_id": "../x" });
        assert!(message_for("Stop", &bad, 3).unwrap().session.is_none());
    }

    #[test]
    fn notification_uses_payload_message_with_fallback() {
        let m = message_for("Notification", &json!({"message": "Allow Bash?"}), 4).unwrap();
        assert_eq!(m.state.as_deref(), Some("needs-input"));
        assert_eq!(m.body.as_deref(), Some("Allow Bash?"));
        assert_eq!(m.pane, 4);
        let m = message_for("Notification", &Value::Null, 4).unwrap();
        assert_eq!(m.body.as_deref(), Some("Claude Code needs your attention"));
    }

    #[test]
    fn activity_comes_from_the_prompt_and_the_tool() {
        let tool = json!({
            "tool_name": "Bash",
            "tool_input": { "command": "cargo test" },
            "cwd": "/p"
        });
        let m = message_for("PreToolUse", &tool, 1).unwrap();
        assert_eq!(m.state.as_deref(), Some("working"));
        assert_eq!(m.activity.as_deref(), Some("Bash: cargo test"));
        let edit = json!({
            "tool_name": "Edit",
            "tool_input": { "file_path": "/p/src/a.rs" },
            "cwd": "/p"
        });
        let m = message_for("PreToolUse", &edit, 1).unwrap();
        assert_eq!(m.activity.as_deref(), Some("Edit: src/a.rs"));
        let unknown = json!({ "tool_name": "mcp__x__y", "tool_input": {} });
        let m = message_for("PreToolUse", &unknown, 1).unwrap();
        assert_eq!(m.activity.as_deref(), Some("mcp__x__y"));
        let prompt = json!({ "prompt": "fix the bug\nplease" });
        let m = message_for("UserPromptSubmit", &prompt, 1).unwrap();
        assert_eq!(m.activity.as_deref(), Some("Prompt: fix the bug"));
        // no tool name, no payload: still working, without activity
        let m = message_for("PreToolUse", &json!({}), 1).unwrap();
        assert_eq!((m.state.as_deref(), m.activity), (Some("working"), None));
        let m = message_for("PreToolUse", &Value::Null, 1).unwrap();
        assert_eq!(m.activity, None);
    }

    #[test]
    fn other_events_carry_no_activity() {
        let payload = json!({
            "tool_name": "Bash",
            "tool_input": { "command": "ls" },
            "prompt": "hi",
            "message": "Allow?"
        });
        for e in ["Stop", "SubagentStop", "SessionEnd", "Notification"] {
            assert_eq!(message_for(e, &payload, 1).unwrap().activity, None, "{e}");
        }
        let start = json!({ "session_id": "abc" });
        assert_eq!(
            message_for("SessionStart", &start, 1).unwrap().activity,
            None
        );
    }

    #[test]
    fn fragment_registers_every_event() {
        let f = settings_fragment();
        for (event, _) in EVENTS {
            let cmd = &f["hooks"][event][0]["hooks"][0]["command"];
            assert!(cmd
                .as_str()
                .unwrap()
                .ends_with(&format!("hook claude {event}")));
        }
        assert_eq!(f["hooks"]["PreToolUse"][0]["matcher"], "*");
    }

    #[test]
    fn merge_preserves_user_settings_and_is_idempotent() {
        let mut s = json!({
            "model": "opus",
            "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "say done" }] }] }
        });
        let added = merge_into(&mut s).unwrap();
        assert_eq!(added.len(), EVENTS.len());
        assert_eq!(s["model"], "opus");
        let stop = s["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["hooks"][0]["command"], "say done");
        let again = merge_into(&mut s).unwrap();
        assert!(again.is_empty());
        assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn merge_rejects_unexpected_shapes() {
        assert!(merge_into(&mut json!([])).is_err());
        assert!(merge_into(&mut json!({"hooks": []})).is_err());
        assert!(merge_into(&mut json!({"hooks": {"Stop": {}}})).is_err());
    }
}
