/// A single command palette action.
#[derive(Debug, Clone)]
pub struct PaletteAction {
    /// Display name shown in the palette.
    pub name: String,
    /// Internal action tag for dispatch.
    pub action: Action,
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
    // Panes
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    // Overlays
    CommandPalette,
    // Window
    ToggleFullscreen,
    Quit,
    // AI / Phase 2
    ToggleAiPanel,
    ToggleAiMode,   // legacy alias — same behaviour as ToggleAiPanel
    EnableAiFeatures,
    DisableAiFeatures,
    ExplainLastOutput,
    FixLastError,
}

impl std::str::FromStr for Action {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "OpenConfigFile"    => Ok(Action::OpenConfigFile),
            "ReloadConfig"      => Ok(Action::ReloadConfig),
            "NewTab"            => Ok(Action::NewTab),
            "CloseTab"          => Ok(Action::CloseTab),
            "SplitHorizontal"   => Ok(Action::SplitHorizontal),
            "SplitVertical"     => Ok(Action::SplitVertical),
            "ClosePane"         => Ok(Action::ClosePane),
            "CommandPalette"    => Ok(Action::CommandPalette),
            "ToggleFullscreen"  => Ok(Action::ToggleFullscreen),
            "Quit"              => Ok(Action::Quit),
            "ToggleAiPanel"     => Ok(Action::ToggleAiPanel),
            "ToggleAiMode"      => Ok(Action::ToggleAiPanel), // alias
            "EnableAiFeatures"  => Ok(Action::EnableAiFeatures),
            "DisableAiFeatures" => Ok(Action::DisableAiFeatures),
            "ExplainLastOutput" => Ok(Action::ExplainLastOutput),
            "FixLastError"      => Ok(Action::FixLastError),
            _                   => Err(()),
        }
    }
}

/// Build the built-in action registry.
pub fn built_in_actions() -> Vec<PaletteAction> {
    vec![
        PaletteAction { name: "Open Config File".into(),        action: Action::OpenConfigFile },
        PaletteAction { name: "Reload Config".into(),           action: Action::ReloadConfig },
        PaletteAction { name: "New Tab".into(),                 action: Action::NewTab },
        PaletteAction { name: "Close Tab".into(),               action: Action::CloseTab },
        PaletteAction { name: "Split Pane Horizontal".into(),   action: Action::SplitHorizontal },
        PaletteAction { name: "Split Pane Vertical".into(),     action: Action::SplitVertical },
        PaletteAction { name: "Close Pane".into(),              action: Action::ClosePane },
        PaletteAction { name: "Toggle Fullscreen".into(),       action: Action::ToggleFullscreen },
        PaletteAction { name: "Quit PetruTerm".into(),          action: Action::Quit },
        // Phase 2 stubs
        PaletteAction { name: "Toggle AI Panel".into(),          action: Action::ToggleAiPanel },
        PaletteAction { name: "Enable AI Features".into(),      action: Action::EnableAiFeatures },
        PaletteAction { name: "Disable AI Features".into(),     action: Action::DisableAiFeatures },
        PaletteAction { name: "Explain Last Output".into(),     action: Action::ExplainLastOutput },
        PaletteAction { name: "Fix Last Error".into(),          action: Action::FixLastError },
    ]
}
