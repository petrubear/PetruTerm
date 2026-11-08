//! Claude Code. Hooks are injected by the `claude` shim (`scripts/shims/claude`) and mapped by
//! [`claude_hooks`](crate::ipc::claude_hooks); this type only puts that mapping behind the
//! shared interface, with behavior unchanged.

use serde_json::Value;

use super::AgentIntegration;
use crate::ipc::{claude_hooks, Message};

pub struct Claude;

impl AgentIntegration for Claude {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn events(&self) -> &'static [&'static str] {
        &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "Notification",
            "Stop",
            "SubagentStop",
            "SessionEnd",
        ]
    }

    fn on_hook(&self, event: &str, payload: &Value, pane: u64) -> Option<Message> {
        claude_hooks::message_for(event, payload, pane)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_case;
    use crate::ipc::integrations::contract::{check_integration, load, PANE};

    #[test]
    fn events_match_the_hooks_the_shim_registers() {
        let shim: Vec<&str> = claude_hooks::EVENTS.iter().map(|(e, _)| *e).collect();
        assert_eq!(Claude.events(), shim.as_slice());
    }

    #[test]
    fn generic_contract() {
        check_integration(&Claude);
    }

    #[test]
    fn fixtures() {
        let i = &Claude;
        contract_case!(i, "claude", "SessionStart", None, None);
        contract_case!(
            i,
            "claude",
            "UserPromptSubmit",
            Some("working"),
            Some("Prompt: fix")
        );
        contract_case!(
            i,
            "claude",
            "PreToolUse",
            Some("working"),
            Some("Bash: cargo test")
        );
        contract_case!(i, "claude", "Notification", Some("needs-input"), None);
        contract_case!(i, "claude", "Stop", Some("done"), None);
        contract_case!(i, "claude", "SessionEnd", Some("idle"), None);
    }

    #[test]
    fn identical_to_the_original_mapping() {
        for name in [
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "Notification",
            "Stop",
            "SessionEnd",
        ] {
            let (event, payload) = load("claude", name);
            assert_eq!(
                Claude.on_hook(&event, &payload, PANE),
                claude_hooks::message_for(&event, &payload, PANE),
                "{name}"
            );
        }
    }
}
