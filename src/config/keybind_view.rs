use super::schema::{Config, KeyBind};

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
        let mut config = Config::default();
        config.keys = vec![
            kb("LEADER", "c", "NewTab"),
            kb("CMD", "k", "ClearScreen"),
            kb("leader", "x", "ClosePane"),
        ];
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
}
