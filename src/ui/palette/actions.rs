use crate::config::Config;
use rust_i18n::t;

/// A single command palette action.
#[derive(Debug, Clone)]
pub struct PaletteAction {
    /// Display name shown in the palette.
    pub name: String,
    /// Internal action tag for dispatch.
    pub action: Action,
    /// Formatted keybind hint shown right-aligned (e.g. "^B c", "Cmd+Q").
    pub keybind: Option<String>,
}

/// All built-in actions for Phase 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // Placeholder — palette item that does nothing when selected.
    Noop,
    // Config
    OpenConfigFile,
    ReloadConfig,
    // Tabs
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    SwitchToTab(usize),
    // Panes
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    FocusPane(crate::ui::panes::FocusDir),
    // Overlays
    CommandPalette,
    // Window
    ToggleFullscreen,
    Quit,
    // AI / Phase 2
    ToggleAiPanel,
    #[allow(dead_code)]
    ToggleAiMode, // legacy alias — same behaviour as ToggleAiPanel
    FocusAiPanel,
    EnableAiFeatures,
    DisableAiFeatures,
    ExplainLastOutput,
    FixLastError,
    UndoLastWrite,
    ToggleStatusBar,
    RenameTab,
    GitCheckout(String),
    ExpandSnippet(String),
    // Phase 3 P3 — Themes
    OpenThemePicker,
    SwitchTheme(String),
}

impl std::str::FromStr for Action {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "OpenConfigFile" => Ok(Action::OpenConfigFile),
            "ReloadConfig" => Ok(Action::ReloadConfig),
            "NewTab" => Ok(Action::NewTab),
            "CloseTab" => Ok(Action::CloseTab),
            "NextTab" => Ok(Action::NextTab),
            "PrevTab" => Ok(Action::PrevTab),
            s if s.starts_with("Tab") => s[3..]
                .parse::<usize>()
                .map(Action::SwitchToTab)
                .map_err(|_| ()),
            "SplitHorizontal" => Ok(Action::SplitHorizontal),
            "SplitVertical" => Ok(Action::SplitVertical),
            "ClosePane" => Ok(Action::ClosePane),
            "FocusPaneLeft" => Ok(Action::FocusPane(crate::ui::panes::FocusDir::Left)),
            "FocusPaneRight" => Ok(Action::FocusPane(crate::ui::panes::FocusDir::Right)),
            "FocusPaneUp" => Ok(Action::FocusPane(crate::ui::panes::FocusDir::Up)),
            "FocusPaneDown" => Ok(Action::FocusPane(crate::ui::panes::FocusDir::Down)),
            "CommandPalette" => Ok(Action::CommandPalette),
            "ToggleFullscreen" => Ok(Action::ToggleFullscreen),
            "Quit" => Ok(Action::Quit),
            "ToggleAiPanel" => Ok(Action::ToggleAiPanel),
            "ToggleAiMode" => Ok(Action::ToggleAiPanel), // alias
            "FocusAiPanel" => Ok(Action::FocusAiPanel),
            "EnableAiFeatures" => Ok(Action::EnableAiFeatures),
            "DisableAiFeatures" => Ok(Action::DisableAiFeatures),
            "ExplainLastOutput" => Ok(Action::ExplainLastOutput),
            "FixLastError" => Ok(Action::FixLastError),
            "UndoLastWrite" => Ok(Action::UndoLastWrite),
            "ToggleStatusBar" => Ok(Action::ToggleStatusBar),
            "RenameTab" => Ok(Action::RenameTab),
            _ => Err(()),
        }
    }
}

/// Build the built-in action list with keybinds resolved from `config`.
pub fn built_in_actions(config: &Config) -> Vec<PaletteAction> {
    // Build a lookup: Action string → formatted keybind label.
    let leader_label = format!("^{}", config.leader.key.to_uppercase());
    let mut keybind_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for kb in &config.keys {
        if kb.mods.eq_ignore_ascii_case("LEADER") {
            keybind_map.insert(kb.action.clone(), format!("{} {}", leader_label, kb.key));
        }
    }

    let kb = |action: &str| -> Option<String> { keybind_map.get(action).cloned() };

    let mut actions = vec![
        PaletteAction {
            name: t!("palette.open_config").to_string(),
            action: Action::OpenConfigFile,
            keybind: None,
        },
        PaletteAction {
            name: t!("palette.reload_config").to_string(),
            action: Action::ReloadConfig,
            keybind: None,
        },
        PaletteAction {
            name: t!("palette.new_tab").to_string(),
            action: Action::NewTab,
            keybind: kb("NewTab"),
        },
        PaletteAction {
            name: t!("palette.close_tab").to_string(),
            action: Action::CloseTab,
            keybind: kb("CloseTab"),
        },
        PaletteAction {
            name: t!("palette.next_tab").to_string(),
            action: Action::NextTab,
            keybind: kb("NextTab"),
        },
        PaletteAction {
            name: t!("palette.prev_tab").to_string(),
            action: Action::PrevTab,
            keybind: kb("PrevTab"),
        },
        PaletteAction {
            name: t!("palette.split_h").to_string(),
            action: Action::SplitHorizontal,
            keybind: kb("SplitHorizontal"),
        },
        PaletteAction {
            name: t!("palette.split_v").to_string(),
            action: Action::SplitVertical,
            keybind: kb("SplitVertical"),
        },
        PaletteAction {
            name: t!("palette.close_pane").to_string(),
            action: Action::ClosePane,
            keybind: kb("ClosePane"),
        },
        PaletteAction {
            name: t!("palette.focus_left").to_string(),
            action: Action::FocusPane(crate::ui::panes::FocusDir::Left),
            keybind: kb("FocusPaneLeft"),
        },
        PaletteAction {
            name: t!("palette.focus_right").to_string(),
            action: Action::FocusPane(crate::ui::panes::FocusDir::Right),
            keybind: kb("FocusPaneRight"),
        },
        PaletteAction {
            name: t!("palette.focus_up").to_string(),
            action: Action::FocusPane(crate::ui::panes::FocusDir::Up),
            keybind: kb("FocusPaneUp"),
        },
        PaletteAction {
            name: t!("palette.focus_down").to_string(),
            action: Action::FocusPane(crate::ui::panes::FocusDir::Down),
            keybind: kb("FocusPaneDown"),
        },
        PaletteAction {
            name: t!("palette.command_palette").to_string(),
            action: Action::CommandPalette,
            keybind: kb("CommandPalette"),
        },
        PaletteAction {
            name: t!("palette.toggle_fullscreen").to_string(),
            action: Action::ToggleFullscreen,
            keybind: None,
        },
        PaletteAction {
            name: t!("palette.quit").to_string(),
            action: Action::Quit,
            keybind: Some("Cmd+Q".into()),
        },
        PaletteAction {
            name: t!("palette.toggle_ai").to_string(),
            action: Action::ToggleAiPanel,
            keybind: kb("ToggleAiPanel"),
        },
        PaletteAction {
            name: t!("palette.enable_ai").to_string(),
            action: Action::EnableAiFeatures,
            keybind: None,
        },
        PaletteAction {
            name: t!("palette.disable_ai").to_string(),
            action: Action::DisableAiFeatures,
            keybind: None,
        },
        PaletteAction {
            name: t!("palette.explain").to_string(),
            action: Action::ExplainLastOutput,
            keybind: kb("ExplainLastOutput"),
        },
        PaletteAction {
            name: t!("palette.fix_error").to_string(),
            action: Action::FixLastError,
            keybind: kb("FixLastError"),
        },
        PaletteAction {
            name: t!("palette.undo_write").to_string(),
            action: Action::UndoLastWrite,
            keybind: kb("UndoLastWrite"),
        },
        PaletteAction {
            name: t!("palette.toggle_status").to_string(),
            action: Action::ToggleStatusBar,
            keybind: None,
        },
        PaletteAction {
            name: t!("palette.rename_tab").to_string(),
            action: Action::RenameTab,
            keybind: kb("RenameTab"),
        },
        PaletteAction {
            name: t!("palette.switch_theme").to_string(),
            action: Action::OpenThemePicker,
            keybind: None,
        },
    ];
    actions.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    actions
}
