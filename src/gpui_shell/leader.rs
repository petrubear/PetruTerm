// `LeaderAction`: every leader-chord action gpui_shell dispatches. Single-key
// ones come from config via `build_leader_map`; the `a`/`e`/`W` sub-prefixes
// are handled in `input.rs`.

use super::panes::FocusDir;
use crate::config::schema::KeyBind;
use std::collections::HashMap;

/// One leader-key action gpui_shell supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaderAction {
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    RenameTab,
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    ZoomPane,
    FocusPane(FocusDir),
    ToggleAiPanel,
    ExplainLastOutput,
    FixLastError,
    UndoLastWrite,
    NewWorkspace,
    CloseWorkspace,
    NextWorkspace,
    PrevWorkspace,
    RenameWorkspace,
    ToggleWorkspaceSidebar,
    OpenCommandPalette,
}

impl TryFrom<&str> for LeaderAction {
    type Error = ();

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "NewTab" => Ok(LeaderAction::NewTab),
            "CloseTab" => Ok(LeaderAction::CloseTab),
            "NextTab" => Ok(LeaderAction::NextTab),
            "PrevTab" => Ok(LeaderAction::PrevTab),
            "RenameTab" => Ok(LeaderAction::RenameTab),
            "SplitHorizontal" => Ok(LeaderAction::SplitHorizontal),
            "SplitVertical" => Ok(LeaderAction::SplitVertical),
            "ClosePane" => Ok(LeaderAction::ClosePane),
            "ZoomPane" => Ok(LeaderAction::ZoomPane),
            "FocusPaneLeft" => Ok(LeaderAction::FocusPane(FocusDir::Left)),
            "FocusPaneRight" => Ok(LeaderAction::FocusPane(FocusDir::Right)),
            "FocusPaneUp" => Ok(LeaderAction::FocusPane(FocusDir::Up)),
            "FocusPaneDown" => Ok(LeaderAction::FocusPane(FocusDir::Down)),
            "ToggleAiPanel" => Ok(LeaderAction::ToggleAiPanel),
            "ExplainLastOutput" => Ok(LeaderAction::ExplainLastOutput),
            "FixLastError" => Ok(LeaderAction::FixLastError),
            "UndoLastWrite" => Ok(LeaderAction::UndoLastWrite),
            "NewWorkspace" => Ok(LeaderAction::NewWorkspace),
            "CloseWorkspace" => Ok(LeaderAction::CloseWorkspace),
            "NextWorkspace" => Ok(LeaderAction::NextWorkspace),
            "PrevWorkspace" => Ok(LeaderAction::PrevWorkspace),
            "RenameWorkspace" => Ok(LeaderAction::RenameWorkspace),
            "ToggleWorkspaceSidebar" => Ok(LeaderAction::ToggleWorkspaceSidebar),
            "CommandPalette" => Ok(LeaderAction::OpenCommandPalette),
            _ => Err(()),
        }
    }
}

/// Build the single-key leader dispatch table from `config.keys`'s
/// `LEADER`-scoped bindings (`leader_bindings_view(config).bindings`).
///
/// `z` (ZoomPane), `w` (NewWorkspace) and `s` (ToggleWorkspaceSidebar) are
/// seeded as defaults via `entry().or_insert`, so a config binding for
/// the same key overrides them.
pub fn build_leader_map(bindings: &[KeyBind]) -> HashMap<String, LeaderAction> {
    let mut map = HashMap::new();
    for kb in bindings {
        if let Ok(action) = LeaderAction::try_from(kb.action.as_str()) {
            map.insert(kb.key.clone(), action);
        }
    }
    map.entry("z".to_string()).or_insert(LeaderAction::ZoomPane);
    map.entry("w".to_string())
        .or_insert(LeaderAction::NewWorkspace);
    map.entry("s".to_string())
        .or_insert(LeaderAction::ToggleWorkspaceSidebar);
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kb(key: &str, action: &str) -> KeyBind {
        KeyBind {
            mods: "LEADER".into(),
            key: key.into(),
            action: action.into(),
        }
    }

    #[test]
    fn parses_all_action_strings() {
        assert_eq!(LeaderAction::try_from("NewTab"), Ok(LeaderAction::NewTab));
        assert_eq!(
            LeaderAction::try_from("CloseTab"),
            Ok(LeaderAction::CloseTab)
        );
        assert_eq!(LeaderAction::try_from("NextTab"), Ok(LeaderAction::NextTab));
        assert_eq!(LeaderAction::try_from("PrevTab"), Ok(LeaderAction::PrevTab));
        assert_eq!(
            LeaderAction::try_from("RenameTab"),
            Ok(LeaderAction::RenameTab)
        );
        assert_eq!(
            LeaderAction::try_from("SplitHorizontal"),
            Ok(LeaderAction::SplitHorizontal)
        );
        assert_eq!(
            LeaderAction::try_from("SplitVertical"),
            Ok(LeaderAction::SplitVertical)
        );
        assert_eq!(
            LeaderAction::try_from("ClosePane"),
            Ok(LeaderAction::ClosePane)
        );
        assert_eq!(
            LeaderAction::try_from("ZoomPane"),
            Ok(LeaderAction::ZoomPane)
        );
        assert_eq!(
            LeaderAction::try_from("FocusPaneLeft"),
            Ok(LeaderAction::FocusPane(FocusDir::Left))
        );
        assert_eq!(
            LeaderAction::try_from("FocusPaneRight"),
            Ok(LeaderAction::FocusPane(FocusDir::Right))
        );
        assert_eq!(
            LeaderAction::try_from("FocusPaneUp"),
            Ok(LeaderAction::FocusPane(FocusDir::Up))
        );
        assert_eq!(
            LeaderAction::try_from("FocusPaneDown"),
            Ok(LeaderAction::FocusPane(FocusDir::Down))
        );
        assert_eq!(
            LeaderAction::try_from("ToggleAiPanel"),
            Ok(LeaderAction::ToggleAiPanel)
        );
        assert_eq!(
            LeaderAction::try_from("NewWorkspace"),
            Ok(LeaderAction::NewWorkspace)
        );
        assert_eq!(
            LeaderAction::try_from("CloseWorkspace"),
            Ok(LeaderAction::CloseWorkspace)
        );
        assert_eq!(
            LeaderAction::try_from("NextWorkspace"),
            Ok(LeaderAction::NextWorkspace)
        );
        assert_eq!(
            LeaderAction::try_from("PrevWorkspace"),
            Ok(LeaderAction::PrevWorkspace)
        );
        assert_eq!(
            LeaderAction::try_from("RenameWorkspace"),
            Ok(LeaderAction::RenameWorkspace)
        );
        assert_eq!(
            LeaderAction::try_from("ToggleWorkspaceSidebar"),
            Ok(LeaderAction::ToggleWorkspaceSidebar)
        );
        assert_eq!(LeaderAction::try_from("NotAnAction"), Err(()));
    }

    #[test]
    fn build_leader_map_matches_default_keybinds_lua() {
        // Mirrors config/default/keybinds.lua's LEADER-scoped entries.
        let bindings = vec![
            kb("c", "NewTab"),
            kb("&", "CloseTab"),
            kb("n", "NextTab"),
            kb("b", "PrevTab"),
            kb(",", "RenameTab"),
            kb("%", "SplitHorizontal"),
            kb("\"", "SplitVertical"),
            kb("x", "ClosePane"),
            kb("h", "FocusPaneLeft"),
            kb("j", "FocusPaneDown"),
            kb("k", "FocusPaneUp"),
            kb("l", "FocusPaneRight"),
            kb("o", "CommandPalette"),
        ];
        let map = build_leader_map(&bindings);
        assert_eq!(map.get("c"), Some(&LeaderAction::NewTab));
        assert_eq!(map.get("&"), Some(&LeaderAction::CloseTab));
        assert_eq!(map.get("n"), Some(&LeaderAction::NextTab));
        assert_eq!(map.get("b"), Some(&LeaderAction::PrevTab));
        assert_eq!(map.get(","), Some(&LeaderAction::RenameTab));
        assert_eq!(map.get("%"), Some(&LeaderAction::SplitHorizontal));
        assert_eq!(map.get("\""), Some(&LeaderAction::SplitVertical));
        assert_eq!(map.get("x"), Some(&LeaderAction::ClosePane));
        assert_eq!(map.get("h"), Some(&LeaderAction::FocusPane(FocusDir::Left)));
        assert_eq!(map.get("j"), Some(&LeaderAction::FocusPane(FocusDir::Down)));
        assert_eq!(map.get("k"), Some(&LeaderAction::FocusPane(FocusDir::Up)));
        assert_eq!(
            map.get("l"),
            Some(&LeaderAction::FocusPane(FocusDir::Right))
        );
        // Seeded even though it's absent from the input bindings.
        assert_eq!(map.get("z"), Some(&LeaderAction::ZoomPane));
        // Seeded even though it's absent from the input bindings, same as "z".
        assert_eq!(map.get("w"), Some(&LeaderAction::NewWorkspace));
        assert_eq!(map.get("s"), Some(&LeaderAction::ToggleWorkspaceSidebar));
        assert_eq!(map.get("o"), Some(&LeaderAction::OpenCommandPalette));
    }

    #[test]
    fn unparseable_action_strings_are_skipped() {
        let bindings = vec![kb("o", "SomeUnknownFutureAction")];
        let map = build_leader_map(&bindings);
        assert_eq!(map.get("o"), None);
        assert_eq!(map.len(), 3); // just the seeded "z", "w", and "s"
    }
}
