use crate::config::Config;

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
            name: "Open Config File".into(),
            action: Action::OpenConfigFile,
            keybind: None,
        },
        PaletteAction {
            name: "Reload Config".into(),
            action: Action::ReloadConfig,
            keybind: None,
        },
        PaletteAction {
            name: "New Tab".into(),
            action: Action::NewTab,
            keybind: kb("NewTab"),
        },
        PaletteAction {
            name: "Close Tab".into(),
            action: Action::CloseTab,
            keybind: kb("CloseTab"),
        },
        PaletteAction {
            name: "Next Tab".into(),
            action: Action::NextTab,
            keybind: kb("NextTab"),
        },
        PaletteAction {
            name: "Previous Tab".into(),
            action: Action::PrevTab,
            keybind: kb("PrevTab"),
        },
        PaletteAction {
            name: "Split Pane Horizontal".into(),
            action: Action::SplitHorizontal,
            keybind: kb("SplitHorizontal"),
        },
        PaletteAction {
            name: "Split Pane Vertical".into(),
            action: Action::SplitVertical,
            keybind: kb("SplitVertical"),
        },
        PaletteAction {
            name: "Close Pane".into(),
            action: Action::ClosePane,
            keybind: kb("ClosePane"),
        },
        PaletteAction {
            name: "Focus Pane Left".into(),
            action: Action::FocusPane(crate::ui::panes::FocusDir::Left),
            keybind: kb("FocusPaneLeft"),
        },
        PaletteAction {
            name: "Focus Pane Right".into(),
            action: Action::FocusPane(crate::ui::panes::FocusDir::Right),
            keybind: kb("FocusPaneRight"),
        },
        PaletteAction {
            name: "Focus Pane Up".into(),
            action: Action::FocusPane(crate::ui::panes::FocusDir::Up),
            keybind: kb("FocusPaneUp"),
        },
        PaletteAction {
            name: "Focus Pane Down".into(),
            action: Action::FocusPane(crate::ui::panes::FocusDir::Down),
            keybind: kb("FocusPaneDown"),
        },
        PaletteAction {
            name: "Command Palette".into(),
            action: Action::CommandPalette,
            keybind: kb("CommandPalette"),
        },
        PaletteAction {
            name: "Toggle Fullscreen".into(),
            action: Action::ToggleFullscreen,
            keybind: None,
        },
        PaletteAction {
            name: "Quit PetruTerm".into(),
            action: Action::Quit,
            keybind: Some("Cmd+Q".into()),
        },
        // Phase 2 AI actions
        PaletteAction {
            name: "Toggle AI Panel".into(),
            action: Action::ToggleAiPanel,
            keybind: kb("ToggleAiPanel"),
        },
        PaletteAction {
            name: "Enable AI Features".into(),
            action: Action::EnableAiFeatures,
            keybind: None,
        },
        PaletteAction {
            name: "Disable AI Features".into(),
            action: Action::DisableAiFeatures,
            keybind: None,
        },
        PaletteAction {
            name: "Explain Last Output".into(),
            action: Action::ExplainLastOutput,
            keybind: kb("ExplainLastOutput"),
        },
        PaletteAction {
            name: "Fix Last Error".into(),
            action: Action::FixLastError,
            keybind: kb("FixLastError"),
        },
        PaletteAction {
            name: "Undo Last Write".into(),
            action: Action::UndoLastWrite,
            keybind: kb("UndoLastWrite"),
        },
        // Phase 3 UI actions
        PaletteAction {
            name: "Toggle Status Bar".into(),
            action: Action::ToggleStatusBar,
            keybind: None,
        },
        PaletteAction {
            name: "Rename Tab".into(),
            action: Action::RenameTab,
            keybind: kb("RenameTab"),
        },
        PaletteAction {
            name: "Switch Theme\u{2026}".into(),
            action: Action::OpenThemePicker,
            keybind: None,
        },
    ];
    actions.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    actions
}
