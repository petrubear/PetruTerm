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
    // Window
    ToggleFullscreen,
    Quit,
    // Phase 2 (registered here as stubs so palette works before Phase 2)
    ToggleAiMode,
    EnableAiFeatures,
    DisableAiFeatures,
    ExplainLastOutput,
    FixLastError,
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
        PaletteAction { name: "Toggle AI Mode".into(),          action: Action::ToggleAiMode },
        PaletteAction { name: "Enable AI Features".into(),      action: Action::EnableAiFeatures },
        PaletteAction { name: "Disable AI Features".into(),     action: Action::DisableAiFeatures },
        PaletteAction { name: "Explain Last Output".into(),     action: Action::ExplainLastOutput },
        PaletteAction { name: "Fix Last Error".into(),          action: Action::FixLastError },
    ]
}
