//! Agent integrations: one implementation per coding agent that reports its state to
//! PetruTerm through hooks. Everything agent-specific lives in `integrations/<agent>.rs`; the
//! tracker, rows, cards and socket are agent-neutral. See
//! `specs/004-agent-integrations/contracts/integration-trait.md`.
//!
//! # Adding an agent
//!
//! 1. Create `integrations/<agent>.rs` with a unit struct implementing [`AgentIntegration`]:
//!    at least `id`, `events` and `on_hook` (map one hook call to a [`Message`], or `None`).
//! 2. Add one line to [`all`].
//! 3. Add fixture payloads under `tests/fixtures/agents/<agent>/<Event>[_variant].json` and a
//!    test that calls [`contract::check_integration`] plus one `contract_case!` per fixture.
//! 4. If the agent keeps hooks in a file, also implement `binaries`, `hook_file`, `install`
//!    and `remove` (see `hook_files` for the shared helpers).
//! 5. Make sure the agent is in `default_agents()` (or the user's `agents.recognized`) so its
//!    pane is detected.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::Message;
use crate::term::agents::sanitize_session;

pub mod agy;
pub mod claude;
pub mod codex;
pub mod kiro;

#[cfg(test)]
mod equivalence;

/// One agent's hook contract. Implementations are stateless.
pub trait AgentIntegration: Sync {
    /// Name used on the command line: `petruterm hook <id> <event>`.
    fn id(&self) -> &'static str;

    /// Hook events the integration registers (documentation of the contract; the generic
    /// contract test runs hostile payloads through each of them).
    #[allow(dead_code)] // only the tests read it
    fn events(&self) -> &'static [&'static str];

    /// Map one hook invocation (event name, stdin payload, pane uid) to a report, or `None`.
    /// Must never panic on any JSON.
    fn on_hook(&self, event: &str, payload: &Value, pane: u64) -> Option<Message>;

    /// Exact text the hook must print for this event (some agents require JSON output).
    fn hook_stdout(&self, _event: &str) -> Option<&'static str> {
        None
    }

    /// Executables whose presence on `PATH` makes `hooks setup` install for this agent.
    fn binaries(&self) -> &'static [&'static str] {
        &[]
    }

    /// The agent's global hook file, when it has one we manage.
    fn hook_file(&self, _home: &Path) -> Option<PathBuf> {
        None
    }

    /// Merge our entries into the parsed hook document. Returns how many were added.
    fn install(&self, _doc: &mut Value) -> Result<usize, String> {
        Ok(0)
    }

    /// Strip our entries (and only ours) from the document. Returns how many were removed.
    fn remove(&self, _doc: &mut Value) -> usize {
        0
    }

    /// How many of our entries the document holds.
    fn installed(&self, _doc: &Value) -> usize {
        0
    }

    /// True when nothing of the user's is left in the document (so the file may be deleted).
    fn is_empty(&self, doc: &Value) -> bool {
        doc.as_object().is_none_or(|o| o.is_empty())
    }

    /// An extra line printed after a successful setup.
    fn setup_note(&self) -> Option<&'static str> {
        None
    }
}

/// Every registered integration.
pub fn all() -> &'static [&'static dyn AgentIntegration] {
    static ALL: [&dyn AgentIntegration; 4] =
        [&claude::Claude, &kiro::Kiro, &codex::Codex, &agy::Agy];
    &ALL
}

/// The integration with this id.
pub fn find(id: &str) -> Option<&'static dyn AgentIntegration> {
    find_in(all(), id)
}

/// Lookup in an explicit registry (tests use their own).
pub fn find_in<'a>(
    registry: &[&'a dyn AgentIntegration],
    id: &str,
) -> Option<&'a dyn AgentIntegration> {
    registry.iter().copied().find(|i| i.id() == id)
}

/// A state report: `title` is the agent's display name (used for notifications).
pub fn report(
    pane: u64,
    title: &str,
    state: &str,
    activity: Option<String>,
    body: Option<String>,
    session: Option<String>,
) -> Message {
    Message {
        pane,
        state: Some(state.into()),
        title: Some(title.into()),
        body,
        // Activity only means something while working.
        activity: if state == "working" { activity } else { None },
        session,
        ..Message::default()
    }
}

/// A report that only announces the session id.
pub fn session_only(pane: u64, session: Option<String>) -> Option<Message> {
    session.map(|session| Message {
        pane,
        session: Some(session),
        ..Message::default()
    })
}

/// The hook command for agents whose hooks must print nothing: a no-op outside PetruTerm
/// (`PETRUTERM_BIN` unset), otherwise `petruterm hook <agent> <event>`.
pub fn plain_hook_command(agent: &str, event: &str) -> String {
    format!(
        r#"[ -z "${MARKER}" ] || "${MARKER}" hook {agent} {event}"#,
        MARKER = super::hook_files::MARKER
    )
}

/// A string field of a JSON object, or `None` for anything else.
pub fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// A well-formed session id from a payload field.
pub fn session_field(v: &Value, key: &str) -> Option<String> {
    str_field(v, key).and_then(sanitize_session)
}

#[cfg(test)]
pub(crate) mod contract {
    //! The checks every integration must pass (contracts/integration-trait.md).

    use super::*;
    use std::path::Path;

    pub const PANE: u64 = 7;

    /// `name` is a fixture file name without extension: `<Event>` or `<Event>_<variant>`.
    pub fn load(agent: &str, name: &str) -> (String, Value) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/agents")
            .join(agent)
            .join(format!("{name}.json"));
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()));
        let value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("fixture {} is not JSON: {e}", path.display()));
        (name.split('_').next().unwrap().to_string(), value)
    }

    /// Check one report against the shared rules.
    pub fn assert_report_shape(m: &Message) {
        assert_eq!(m.pane, PANE);
        if let Some(a) = &m.activity {
            assert_eq!(
                m.state.as_deref(),
                Some("working"),
                "activity only with working"
            );
            assert!(a.chars().count() <= crate::term::agents::MAX_ACTIVITY_CHARS);
            assert!(!a.chars().any(char::is_control));
        }
        if let Some(s) = &m.state {
            assert!(crate::ipc::parse_state(s).is_some(), "bad state {s}");
        }
    }

    /// Run a fixture and compare the state and the start of the activity.
    pub fn expect(
        i: &dyn AgentIntegration,
        agent: &str,
        name: &str,
        state: Option<&str>,
        activity_prefix: Option<&str>,
    ) {
        let (event, payload) = load(agent, name);
        let got = i.on_hook(&event, &payload, PANE);
        match (state, &got) {
            (None, None) => return,
            (None, Some(m)) => {
                assert!(m.state.is_none(), "{name}: expected no state, got {m:?}");
                return;
            }
            (Some(_), None) => panic!("{name}: expected a report"),
            _ => {}
        }
        let m = got.unwrap();
        assert_report_shape(&m);
        assert_eq!(m.state.as_deref(), state, "{name}");
        match activity_prefix {
            Some(p) => assert!(
                m.activity.as_deref().is_some_and(|a| a.starts_with(p)),
                "{name}: activity {:?} should start with {p:?}",
                m.activity
            ),
            None => assert_eq!(m.activity, None, "{name}"),
        }
    }

    /// `contract_case!(integration, "agent", "Fixture_name", Some("working"), Some("Bash: "))`
    #[macro_export]
    macro_rules! contract_case {
        ($i:expr, $agent:expr, $name:expr, $state:expr, $act:expr) => {
            $crate::ipc::integrations::contract::expect($i, $agent, $name, $state, $act)
        };
    }

    /// Garbage and hostile payloads for every declared event never panic.
    pub fn check_garbage(i: &dyn AgentIntegration) {
        let mut nested = Value::Null;
        for _ in 0..400 {
            nested = Value::Array(vec![nested]);
        }
        let hostile = [
            Value::Null,
            serde_json::json!(5),
            serde_json::json!([1, 2, 3]),
            serde_json::json!("x".repeat(2_000_000)),
            serde_json::json!({"tool_name": 5, "tool_input": "x", "prompt": [], "session_id": {}}),
            serde_json::json!({"toolCall": 3, "lastUserInput": null, "fullyIdle": "yes"}),
            nested,
        ];
        for event in i.events().iter().copied().chain(["NoSuchEvent"]) {
            for payload in &hostile {
                if let Some(m) = i.on_hook(event, payload, PANE) {
                    assert_report_shape(&m);
                }
            }
        }
        assert!(i.on_hook("NoSuchEvent", &Value::Null, PANE).is_none());
    }

    /// Declared stdout is valid JSON; undeclared events print nothing.
    pub fn check_stdout(i: &dyn AgentIntegration) {
        for event in i.events() {
            if let Some(out) = i.hook_stdout(event) {
                serde_json::from_str::<Value>(out)
                    .unwrap_or_else(|e| panic!("{} {event}: stdout is not JSON: {e}", i.id()));
            }
        }
        assert!(i.hook_stdout("NoSuchEvent").is_none());
    }

    /// The generic checks, run once per integration.
    pub fn check_integration(i: &dyn AgentIntegration) {
        assert!(!i.id().is_empty() && i.id() == i.id().to_ascii_lowercase());
        assert!(!i.events().is_empty());
        check_garbage(i);
        check_stdout(i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_resolves_ids_and_rejects_unknown_ones() {
        assert!(find("claude").is_some());
        assert!(find("nope").is_none());
        assert!(find("").is_none());
    }

    #[test]
    fn registry_ids_are_unique_and_lowercase() {
        let mut ids: Vec<_> = all().iter().map(|i| i.id()).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
        assert!(ids.iter().all(|i| *i == i.to_ascii_lowercase()));
    }

    #[test]
    fn every_registered_integration_passes_the_generic_checks() {
        for i in all() {
            contract::check_integration(*i);
        }
    }

    /// A fictional fifth agent written exactly from the recipe in the module doc: one unit
    /// struct, two events, nothing else in the crate changes (SC-007).
    struct Fake;

    impl AgentIntegration for Fake {
        fn id(&self) -> &'static str {
            "fake"
        }
        fn events(&self) -> &'static [&'static str] {
            &["Begin", "Finish"]
        }
        fn on_hook(&self, event: &str, payload: &Value, pane: u64) -> Option<Message> {
            let session = session_field(payload, "who");
            match event {
                "Begin" => Some(report(
                    pane,
                    "Fake",
                    "working",
                    str_field(payload, "task")
                        .and_then(|t| crate::term::agents::clean_activity(&format!("Task: {t}"))),
                    None,
                    session,
                )),
                "Finish" => Some(report(pane, "Fake", "done", None, None, session)),
                _ => None,
            }
        }
    }

    #[test]
    fn a_new_agent_needs_one_implementation_and_fixtures() {
        let registry: [&dyn AgentIntegration; 2] = [&claude::Claude, &Fake];
        let i = find_in(&registry, "fake").expect("registered");
        contract::check_integration(i);
        crate::contract_case!(
            i,
            "fake",
            "Begin",
            Some("working"),
            Some("Task: index the repo")
        );
        crate::contract_case!(i, "fake", "Finish", Some("done"), None);
        assert!(find_in(&registry, "other").is_none());
        // the real registry is untouched by the test registry
        assert!(find("fake").is_none());
        // hook output defaults: nothing printed, no install
        assert_eq!(i.hook_stdout("Begin"), None);
        assert!(i.binaries().is_empty() && i.hook_file(std::path::Path::new("/h")).is_none());
        let mut doc = serde_json::json!({});
        assert_eq!(i.install(&mut doc).unwrap(), 0);
        assert_eq!(i.remove(&mut doc), 0);
    }

    #[test]
    fn report_keeps_activity_only_for_working() {
        let m = report(1, "X", "working", Some("a".into()), None, None);
        assert_eq!(m.activity.as_deref(), Some("a"));
        let m = report(1, "X", "done", Some("a".into()), None, None);
        assert_eq!(m.activity, None);
        assert!(session_only(1, None).is_none());
        assert_eq!(
            session_only(1, Some("s".into()))
                .unwrap()
                .session
                .as_deref(),
            Some("s")
        );
    }
}
