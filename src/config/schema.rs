use serde::{Deserialize, Serialize};

/// Top-level resolved configuration. All Lua config values are deserialized into this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub font: FontConfig,
    pub window: WindowConfig,
    pub colors: ColorScheme,
    pub scrollback_lines: u32,
    pub enable_scroll_bar: bool,
    pub max_fps: u32,
    pub leader: LeaderConfig,
    pub shell: String,
    pub shell_integration: bool,
    pub llm: LlmConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font: FontConfig::default(),
            window: WindowConfig::default(),
            colors: ColorScheme::dracula_pro(),
            scrollback_lines: 100_000,
            enable_scroll_bar: true,
            max_fps: 60,
            leader: LeaderConfig::default(),
            shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into()),
            shell_integration: true,
            llm: LlmConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    /// Primary font family name.
    pub family: String,
    /// Font size in points.
    pub size: f32,
    /// HarfBuzz OpenType feature tags, e.g. ["calt=1", "liga=1", "dlig=1"].
    pub features: Vec<String>,
    /// Fallback font families tried in order when a glyph is not found.
    pub fallbacks: Vec<String>,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            // Monolisa is paid; JetBrains Mono is the CI/default fallback.
            family: "JetBrainsMono Nerd Font Mono".into(),
            size: 15.0,
            features: vec!["calt=1".into(), "liga=1".into(), "dlig=1".into()],
            fallbacks: vec!["Noto Color Emoji".into()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    pub borderless: bool,
    pub initial_width: Option<u32>,
    pub initial_height: Option<u32>,
    pub start_maximized: bool,
    pub title_bar_style: TitleBarStyle,
    pub padding: Padding,
    pub opacity: f32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            borderless: false,
            initial_width: None,
            initial_height: None,
            start_maximized: true,
            title_bar_style: TitleBarStyle::Custom,
            padding: Padding { left: 20, right: 20, top: 30, bottom: 10 },
            opacity: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TitleBarStyle {
    #[default]
    Custom,
    Native,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Padding {
    pub left: u32,
    pub right: u32,
    pub top: u32,
    pub bottom: u32,
}

/// RGBA color scheme for a terminal theme. All values are linear [0.0, 1.0].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorScheme {
    pub foreground: [f32; 4],
    pub background: [f32; 4],
    pub cursor_bg: [f32; 4],
    pub cursor_fg: [f32; 4],
    pub cursor_border: [f32; 4],
    pub selection_bg: [f32; 4],
    pub selection_fg: [f32; 4],
    /// Normal ANSI colors 0-7.
    pub ansi: [[f32; 4]; 8],
    /// Bright ANSI colors 8-15.
    pub brights: [[f32; 4]; 8],
}

impl ColorScheme {
    pub fn dracula_pro() -> Self {
        fn hex(s: &str) -> [f32; 4] {
            let s = s.trim_start_matches('#');
            let r = u8::from_str_radix(&s[0..2], 16).unwrap() as f32 / 255.0;
            let g = u8::from_str_radix(&s[2..4], 16).unwrap() as f32 / 255.0;
            let b = u8::from_str_radix(&s[4..6], 16).unwrap() as f32 / 255.0;
            [r, g, b, 1.0]
        }
        Self {
            foreground:    hex("#f8f8f2"),
            background:    hex("#22212c"),
            cursor_bg:     hex("#9580ff"),
            cursor_fg:     hex("#f8f8f2"),
            cursor_border: hex("#9580ff"),
            selection_bg:  hex("#454158"),
            selection_fg:  hex("#c6c6c2"),
            ansi: [
                hex("#22212c"), hex("#ff9580"), hex("#8aff80"), hex("#ffff80"),
                hex("#9580ff"), hex("#ff80bf"), hex("#80ffea"), hex("#f8f8f2"),
            ],
            brights: [
                hex("#504c67"), hex("#ffaa99"), hex("#a2ff99"), hex("#ffff99"),
                hex("#aa99ff"), hex("#ff99cc"), hex("#99ffee"), hex("#ffffff"),
            ],
        }
    }

    /// Returns the background as a wgpu-compatible Color (linear sRGB).
    pub fn background_wgpu(&self) -> wgpu::Color {
        let [r, g, b, a] = self.background;
        wgpu::Color { r: r as f64, g: g as f64, b: b as f64, a: a as f64 }
    }

    /// Map a terminal color index (0-15) to RGBA.
    pub fn index_color(&self, idx: u8) -> [f32; 4] {
        match idx {
            0..=7  => self.ansi[idx as usize],
            8..=15 => self.brights[(idx - 8) as usize],
            _      => self.foreground,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderConfig {
    pub key: String,
    pub mods: String,
    pub timeout_ms: u64,
}

impl Default for LeaderConfig {
    fn default() -> Self {
        Self {
            key: "b".into(),
            mods: "CTRL".into(),
            timeout_ms: 1000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub context_lines: u32,
    pub features: LlmFeatures,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "openrouter".into(),
            model: "meta-llama/llama-3.1-8b-instruct:free".into(),
            api_key: None,
            base_url: None,
            context_lines: 50,
            features: LlmFeatures::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmFeatures {
    pub nl_to_command: bool,
    pub explain_output: bool,
    pub fix_last_error: bool,
    pub context_chat: bool,
}

impl Default for LlmFeatures {
    fn default() -> Self {
        Self {
            nl_to_command: true,
            explain_output: true,
            fix_last_error: true,
            context_chat: true,
        }
    }
}
