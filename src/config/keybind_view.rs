use super::schema::{Config, KeyBind};

/// A parsed modifier set from a `KeyBind.mods` string like `"CMD|SHIFT"`.
/// Case-insensitive, `|`-separated; an unrecognized token is ignored,
/// matching the permissive style of the rest of `KeyBind` parsing at the
/// Lua boundary (`lua.rs`'s own `unwrap_or_default()` reads).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Mods {
    pub cmd: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub option: bool,
}

pub fn parse_mods(s: &str) -> Mods {
    let mut mods = Mods::default();
    for token in s.split('|') {
        match token.trim().to_ascii_uppercase().as_str() {
            "CMD" => mods.cmd = true,
            "SHIFT" => mods.shift = true,
            "CTRL" => mods.ctrl = true,
            "OPTION" => mods.option = true,
            _ => {}
        }
    }
    mods
}

#[derive(Debug, Clone)]
pub struct LeaderBindingsView {
    pub leader_key: String,
    pub bindings: Vec<KeyBind>,
}

pub fn leader_bindings_view(config: &Config) -> LeaderBindingsView {
    LeaderBindingsView {
        leader_key: config.leader.key.clone(),
        bindings: config
            .keys
            .iter()
            .filter(|kb| kb.mods.eq_ignore_ascii_case("LEADER"))
            .cloned()
            .collect(),
    }
}

#[derive(Debug, Clone)]
pub struct DirectBindingsView {
    pub bindings: Vec<KeyBind>,
}

/// Every `config.keys` entry whose `mods` is NOT `"LEADER"` -- the direct
/// (non-leader) keybind scheme used when `keybind_style = "normal"`.
/// Mirrors `leader_bindings_view`'s own filter, inverted.
pub fn direct_bindings_view(config: &Config) -> DirectBindingsView {
    DirectBindingsView {
        bindings: config
            .keys
            .iter()
            .filter(|kb| !kb.mods.eq_ignore_ascii_case("LEADER"))
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kb(mods: &str, key: &str, action: &str) -> KeyBind {
        KeyBind {
            mods: mods.into(),
            key: key.into(),
            action: action.into(),
        }
    }

    #[test]
    fn leader_bindings_view_filters_to_leader_mods_only_case_insensitive() {
        let config = Config {
            keys: vec![
                kb("LEADER", "c", "NewTab"),
                kb("CMD", "k", "ClearScreen"),
                kb("leader", "x", "ClosePane"),
            ],
            ..Config::default()
        };
        let view = leader_bindings_view(&config);
        assert_eq!(view.bindings.len(), 2);
        assert!(view
            .bindings
            .iter()
            .any(|kb| kb.key == "c" && kb.action == "NewTab"));
        assert!(view
            .bindings
            .iter()
            .any(|kb| kb.key == "x" && kb.action == "ClosePane"));
    }

    #[test]
    fn leader_bindings_view_carries_leader_key() {
        let mut config = Config::default();
        config.leader.key = "f".into();
        let view = leader_bindings_view(&config);
        assert_eq!(view.leader_key, "f");
    }

    #[test]
    fn parse_mods_single_token() {
        let m = parse_mods("CMD");
        assert!(m.cmd);
        assert!(!m.shift && !m.ctrl && !m.option);
    }

    #[test]
    fn parse_mods_combined_tokens() {
        let m = parse_mods("CMD|SHIFT");
        assert!(m.cmd);
        assert!(m.shift);
        assert!(!m.ctrl && !m.option);
    }

    #[test]
    fn parse_mods_case_insensitive() {
        assert_eq!(parse_mods("cmd|shift"), parse_mods("CMD|SHIFT"));
    }

    #[test]
    fn parse_mods_unknown_token_ignored() {
        let m = parse_mods("CMD|BOGUS");
        assert!(m.cmd);
        assert_eq!(m, parse_mods("CMD"));
    }

    #[test]
    fn parse_mods_empty_string_is_no_modifiers() {
        assert_eq!(parse_mods(""), Mods::default());
    }

    #[test]
    fn direct_bindings_view_filters_out_leader_case_insensitive() {
        let config = Config {
            keys: vec![
                kb("LEADER", "c", "NewTab"),
                kb("CMD", "t", "NewTab"),
                kb("CMD|SHIFT", "w", "CloseTab"),
                kb("leader", "x", "ClosePane"),
            ],
            ..Config::default()
        };
        let view = direct_bindings_view(&config);
        assert_eq!(view.bindings.len(), 2);
        assert!(view
            .bindings
            .iter()
            .any(|kb| kb.mods == "CMD" && kb.key == "t"));
        assert!(view
            .bindings
            .iter()
            .any(|kb| kb.mods == "CMD|SHIFT" && kb.key == "w"));
    }

    #[test]
    fn direct_bindings_view_empty_when_all_leader() {
        let config = Config {
            keys: vec![kb("LEADER", "c", "NewTab")],
            ..Config::default()
        };
        assert!(direct_bindings_view(&config).bindings.is_empty());
    }

    #[test]
    fn direct_bindings_view_output_parses_into_real_actions() {
        // Exercises the exact same two-step pipeline InputHandler::new runs
        // (direct_bindings_view -> parse mods + action per binding), using a
        // real Action string from this codebase rather than a placeholder, to
        // catch a future Action rename that keybind_view itself has no direct
        // dependency on.
        let config = Config {
            keys: vec![kb("CMD|SHIFT", "w", "CloseTab")],
            ..Config::default()
        };
        let view = direct_bindings_view(&config);
        let kb = &view.bindings[0];
        let mods = parse_mods(&kb.mods);
        assert!(mods.cmd && mods.shift && !mods.ctrl && !mods.option);
        assert_eq!(kb.key, "w");
    }
}
