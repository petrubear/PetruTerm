//! Claude Code integration: the hook definitions (shared by the `claude` shim and
//! `petruterm hooks setup claude`) and the mapping from a hook event to a [`Message`].

use super::Message;
use serde_json::{json, Map, Value};

/// Hook events we register, with an optional tool matcher.
pub const EVENTS: [(&str, Option<&str>); 6] = [
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
    let (state, body) = match event {
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
        ..Message::default()
    })
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
    fn notification_uses_payload_message_with_fallback() {
        let m = message_for("Notification", &json!({"message": "Allow Bash?"}), 4).unwrap();
        assert_eq!(m.state.as_deref(), Some("needs-input"));
        assert_eq!(m.body.as_deref(), Some("Allow Bash?"));
        assert_eq!(m.pane, 4);
        let m = message_for("Notification", &Value::Null, 4).unwrap();
        assert_eq!(m.body.as_deref(), Some("Claude Code needs your attention"));
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
