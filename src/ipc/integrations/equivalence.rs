//! Equivalent events from different agents must end up as the same card: the tracker state,
//! the shape of the activity and (because the card style is a pure function of the state and
//! the theme) the card appearance.

use serde_json::Value;

use super::contract::{load, PANE};
use super::AgentIntegration;
use crate::ipc::{claude_hooks, Message};
use crate::term::attention::{AgentState, AttentionTracker};

/// (agent id, prompt fixture, tool fixture, finish fixture)
const FLOWS: [(&str, &str, &str, &str); 4] = [
    ("claude", "UserPromptSubmit", "PreToolUse", "Stop"),
    (
        "kiro",
        "UserPromptSubmit",
        "PreToolUse_execute_bash",
        "Stop",
    ),
    ("codex", "UserPromptSubmit", "PreToolUse_Bash", "Stop"),
    (
        "agy",
        "PreInvocation",
        "PreToolUse_run_command",
        "Stop_idle",
    ),
];

fn message(i: &dyn AgentIntegration, agent: &str, fixture: &str) -> Message {
    let (event, payload) = load(agent, fixture);
    i.on_hook(&event, &payload, PANE)
        .unwrap_or_else(|| panic!("{agent} {fixture}: no report"))
}

fn apply(t: &mut AttentionTracker, m: Message) {
    let (_, event) = m.into_event().expect("a state report");
    t.apply(1, event, false, false, std::time::Instant::now());
}

/// (state, whether an activity text is present) after each step of the flow.
fn run(agent: &str, prompt: &str, tool: &str, finish: &str) -> Vec<(AgentState, Option<bool>)> {
    let i = super::find(agent).unwrap();
    let mut t = AttentionTracker::new();
    let mut seen = Vec::new();
    for fixture in [prompt, tool, finish] {
        apply(&mut t, message(i, agent, fixture));
        let p = t.get(1).unwrap();
        seen.push((
            p.state,
            p.activity
                .as_deref()
                .map(|a| a.contains(": ") || !a.is_empty()),
        ));
    }
    seen
}

#[test]
fn every_agent_walks_the_same_states_with_the_same_activity_shape() {
    let want = vec![
        (AgentState::Working, Some(true)),
        (AgentState::Working, Some(true)),
        (AgentState::Done, None),
    ];
    for (agent, prompt, tool, finish) in FLOWS {
        assert_eq!(run(agent, prompt, tool, finish), want, "{agent}");
    }
}

#[test]
fn the_activity_text_has_the_label_colon_target_form() {
    for (agent, prompt, tool, _) in FLOWS {
        let i = super::find(agent).unwrap();
        let p = message(i, agent, prompt).activity.unwrap();
        assert!(p.starts_with("Prompt: "), "{agent}: {p}");
        let t = message(i, agent, tool).activity.unwrap();
        assert!(t.contains(": "), "{agent}: {t}");
    }
}

#[test]
fn a_needs_input_report_looks_the_same_for_claude_and_codex() {
    let mut codex = AttentionTracker::new();
    let mut claude = AttentionTracker::new();
    let i = super::find("codex").unwrap();
    apply(&mut codex, message(i, "codex", "PermissionRequest"));
    let (event, payload): (String, Value) = load("claude", "Notification");
    apply(
        &mut claude,
        claude_hooks::message_for(&event, &payload, PANE).unwrap(),
    );
    assert_eq!(codex.get(1).unwrap().state, AgentState::NeedsInput);
    assert_eq!(claude.get(1).unwrap().state, AgentState::NeedsInput);
    assert!(codex.get(1).unwrap().last_message.is_some());
    assert!(claude.get(1).unwrap().last_message.is_some());
    assert!(codex.get(1).unwrap().unread && claude.get(1).unwrap().unread);
}
