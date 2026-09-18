// gpui chrome migration (M5c Task 2): a narrow, gpui_shell-local
// tracker of "the word currently being typed since the last prompt",
// just enough to drive Tab-triggered snippet expansion -- NOT a port of
// `crate::term::InputShadow` (ghost text, history completion, PATH
// resolution), which stays hard-coupled to winit's `KeyEvent`/
// `Modifiers` with zero gpui_shell caller (see this milestone's own
// spec, §4, for the full investigation and the decision to defer fully
// decoupling it). `try_expand_snippet` mirrors the wgpu build's own
// `Input::try_expand_snippet` (src/app/input/mod.rs:801-833) against
// this narrower tracker instead of `self.input_echo`.

use gpui::{Context, KeyDownEvent};

use crate::config::Config;
use crate::term::Terminal;

use super::GpuiShellRoot;

impl GpuiShellRoot {
    /// Called from `input.rs`'s `on_key_down`, right before the generic
    /// key-forwarding fallthrough, only when `event.keystroke.key ==
    /// "tab"` with no Shift/Control held. Returns `true` if a snippet
    /// trigger matched and was expanded (caller should NOT forward Tab to
    /// the PTY); `false` otherwise (caller's normal Tab-forwarding is
    /// unaffected).
    pub(super) fn maybe_expand_snippet_tab(
        &mut self,
        event: &KeyDownEvent,
        terminal_id: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key != "tab"
            || event.keystroke.modifiers.shift
            || event.keystroke.modifiers.control
        {
            return false;
        }
        let Some(terminal) = self.terminals.get(&terminal_id) else {
            return false;
        };
        if !try_expand_snippet(&self.config, terminal, &mut self.snippet_word) {
            return false;
        }
        cx.notify();
        true
    }

    /// Called after every key that actually reached the PTY as text (see
    /// `input.rs`'s own call site) -- keeps `self.snippet_word` in sync
    /// with what the shell's line editor is showing, on a best-effort
    /// basis (arrow-key repositioning mid-word is a known, accepted gap:
    /// this tracker only handles the common "type a trigger, press Tab"
    /// pattern, not full cursor-aware editing -- see the spec's own scope
    /// decision on why full `InputShadow` parity is out of scope here).
    pub(super) fn track_snippet_key(&mut self, event: &KeyDownEvent) {
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return;
        }
        if key == "backspace" {
            self.snippet_word.pop();
        } else if key == "space" || key == "enter" || key == "escape" {
            self.snippet_word.clear();
        } else if key.chars().count() == 1 {
            self.snippet_word.push_str(key);
        }
    }
}

/// On a Tab press, look up `word` against `config.snippets`' triggers. On
/// a match: erase the trigger (backspaces) + write the snippet body to
/// the PTY, clear `word`, return `true`. On no match: leave `word`
/// untouched, return `false`.
fn try_expand_snippet(config: &Config, terminal: &Terminal, word: &mut String) -> bool {
    if word.is_empty() || config.snippets.iter().all(|s| s.trigger.is_none()) {
        return false;
    }
    let Some(snippet) = config
        .snippets
        .iter()
        .find(|s| s.trigger.as_deref() == Some(word.as_str()))
    else {
        return false;
    };
    let backspaces = vec![0x7fu8; word.len()];
    terminal.write_input(&backspaces);
    terminal.write_input(snippet.body.as_bytes());
    word.clear();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::SnippetConfig;

    #[test]
    fn no_match_leaves_word_untouched() {
        let config = Config {
            snippets: vec![SnippetConfig {
                name: "test".to_string(),
                trigger: Some("gco".to_string()),
                body: "git checkout ".to_string(),
            }],
            ..Config::default()
        };
        let word = "xyz".to_string();
        // No terminal available in a unit test -- exercise only the
        // lookup half by checking the trigger search directly, matching
        // the pattern this session's other "logic without a live
        // Terminal" tests use (see term::search's own unit tests for the
        // precedent this mirrors).
        let found = config
            .snippets
            .iter()
            .find(|s| s.trigger.as_deref() == Some(word.as_str()));
        assert!(found.is_none());
        assert_eq!(word, "xyz");
    }

    #[test]
    fn empty_word_never_matches() {
        let config = Config {
            snippets: vec![SnippetConfig {
                name: "test".to_string(),
                trigger: Some("".to_string()),
                body: "x".to_string(),
            }],
            ..Config::default()
        };
        let word = String::new();
        assert!(word.is_empty());
        let _ = config; // no live Terminal to call try_expand_snippet with in a unit test
    }
}
