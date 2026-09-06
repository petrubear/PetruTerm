// gpui chrome migration (M2 Task 4): leader-key chorded dispatch.
//
// `LeaderAction` is a deliberately narrower type than the wgpu app's own
// `Action` enum (`src/ui/palette/actions.rs`) -- it covers exactly the
// eleven leader-key actions ported so far (tabs, splits, pane focus/zoom/
// close, plus M3b's `ToggleAiPanel`). The 'e' (explorer) and 'W' (workspace)
// leader sub-prefixes and the command palette itself are still out of scope.
//
// `leader_map` (built once in `GpuiShellRoot::new` via `build_leader_map`)
// is the data-driven half: it turns `config.keys`'s `LEADER`-scoped
// `KeyBind`s (via `config::keybind_view::leader_bindings_view`) into this
// enum, the same string-matching shape as `Action::from_str`
// (`src/ui/palette/actions.rs:71-113`).
//
// M3b adds `ToggleAiPanel`, the milestone this doc comment used to say would
// add AI/workspace/sidebar sub-prefixes. It is NOT seeded into
// `leader_map` the way `ZoomPane`'s "z" is: `Leader a a` is a two-key
// sub-prefix chord (`a` then `a`), and this map is keyed by a single string.
// `input.rs`'s leader dispatch handles the `a`-prefix continuation directly,
// constructing `LeaderAction::ToggleAiPanel` itself rather than looking it up
// here. `TryFrom<&str>` still parses the string for consistency with every
// other variant (and in case a future config surface ever needs to name it),
// but nothing in this milestone calls it that way.

use super::panes::FocusDir;
use crate::config::schema::KeyBind;
use std::collections::HashMap;

/// One leader-key action this milestone supports. See this module's doc
/// comment for why the set stops here.
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
    NewWorkspace,
    CloseWorkspace,
    NextWorkspace,
    PrevWorkspace,
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
            "NewWorkspace" => Ok(LeaderAction::NewWorkspace),
            "CloseWorkspace" => Ok(LeaderAction::CloseWorkspace),
            "NextWorkspace" => Ok(LeaderAction::NextWorkspace),
            "PrevWorkspace" => Ok(LeaderAction::PrevWorkspace),
            _ => Err(()),
        }
    }
}

/// Build the single-key leader dispatch table from `config.keys`'s
/// `LEADER`-scoped bindings (`leader_bindings_view(config).bindings`).
///
/// `Leader z` (zoom) is seeded in unconditionally: unlike every other
/// binding here, it has no `{ mods = "LEADER", key = "z", ... }` entry in
/// `config/default/keybinds.lua` -- the wgpu app dispatches it as a
/// hardcoded single key too (`src/app/input/mod.rs`'s `s.as_str() == "z"`
/// branch), not through its own config-driven `leader_map`. Seeded via
/// `entry().or_insert` rather than an unconditional `insert` so a future
/// config binding for "z" (should one ever be added) overrides this
/// default instead of racing it based on map-insertion order.
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
    }

    #[test]
    fn unparseable_action_strings_are_skipped() {
        let bindings = vec![kb("o", "CommandPalette")];
        let map = build_leader_map(&bindings);
        assert_eq!(map.get("o"), None);
        assert_eq!(map.len(), 2); // just the seeded "z" and "w"
    }
}
