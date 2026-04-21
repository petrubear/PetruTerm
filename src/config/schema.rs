use secrecy::SecretString;
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
    pub keys: Vec<KeyBind>,
    pub snippets: Vec<SnippetConfig>,
    pub shell: String,
    pub shell_integration: bool,
    pub llm: LlmConfig,
    pub status_bar: StatusBarConfig,
    pub keyboard: KeyboardConfig,
}

/// Keyboard behaviour options.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeyboardConfig {
    /// When `true`, the Option/Alt key sends an ESC prefix (Meta key — useful for
    /// Emacs / readline Alt+letter shortcuts).
    /// When `false` (default), Option acts as a compose key and the OS-composed
    /// character is sent as-is — correct for non-US keyboards (Spanish, ISO, etc.)
    /// where characters like `{`, `}`, `@`, `#` require Option+key.
    pub option_as_meta: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusBarConfig {
    pub enabled: bool,
    pub position: StatusBarPosition,
    pub style: StatusBarStyle,
}

impl Default for StatusBarConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            position: StatusBarPosition::Bottom,
            style: StatusBarStyle::Plain,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatusBarPosition {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StatusBarStyle {
    /// Plain text separators: ` › ` between left segments, ` │ ` between right segments.
    #[default]
    Plain,
    /// Nerd Font powerline arrows:  (U+E0B0) for left,  (U+E0B2) for right.
    Powerline,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font: FontConfig::default(),
            window: WindowConfig::default(),
            colors: ColorScheme::dracula_pro(),
            scrollback_lines: 5_000,
            enable_scroll_bar: true,
            max_fps: 60,
            leader: LeaderConfig::default(),
            keys: vec![],
            snippets: vec![],
            shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into()),
            shell_integration: true,
            llm: LlmConfig::default(),
            status_bar: StatusBarConfig::default(),
            keyboard: KeyboardConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    /// Primary font family name.
    pub family: String,
    /// Font size in points.
    pub size: f32,
    /// Line height multiplier (1.0 = no extra leading, 1.2 = 20% extra).
    pub line_height: f32,
    /// HarfBuzz OpenType feature tags, e.g. ["calt=1", "liga=1", "dlig=1"].
    pub features: Vec<String>,
    /// Fallback font families tried in order when a glyph is not found.
    pub fallbacks: Vec<String>,
    /// Enable LCD subpixel antialiasing (FreeType LCD mode, 3× horizontal resolution).
    pub lcd_antialiasing: bool,
    /// Font file path for LCD AA. None means LCD AA is disabled or font couldn't be located.
    pub font_path: Option<std::path::PathBuf>,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "JetBrainsMono Nerd Font Mono".into(),
            size: 15.0,
            line_height: 1.2,
            features: vec!["calt=1".into(), "liga=1".into(), "dlig=1".into()],
            fallbacks: vec!["Noto Color Emoji".into()],
            lcd_antialiasing: false,
            font_path: None,
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
            padding: Padding {
                left: 20,
                right: 20,
                top: 5,   // titlebar (TITLEBAR_HEIGHT=30) handles traffic lights clearance
                bottom: 10,
            },
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
            foreground: hex("#f8f8f2"),
            background: hex("#22212c"),
            cursor_bg: hex("#9580ff"),
            cursor_fg: hex("#f8f8f2"),
            cursor_border: hex("#9580ff"),
            selection_bg: hex("#454158"),
            selection_fg: hex("#c6c6c2"),
            ansi: [
                hex("#22212c"),
                hex("#ff9580"),
                hex("#8aff80"),
                hex("#ffff80"),
                hex("#9580ff"),
                hex("#ff80bf"),
                hex("#80ffea"),
                hex("#f8f8f2"),
            ],
            brights: [
                hex("#504c67"),
                hex("#ffaa99"),
                hex("#a2ff99"),
                hex("#ffff99"),
                hex("#aa99ff"),
                hex("#ff99cc"),
                hex("#99ffee"),
                hex("#ffffff"),
            ],
        }
    }

    /// Returns the background as a wgpu-compatible Color (linear sRGB).
    pub fn background_wgpu(&self) -> wgpu::Color {
        let [r, g, b, a] = self.background;
        wgpu::Color {
            r: r as f64,
            g: g as f64,
            b: b as f64,
            a: a as f64,
        }
    }

    /// Map a terminal color index (0-15) to RGBA.
    pub fn index_color(&self, idx: u8) -> [f32; 4] {
        match idx {
            0..=7 => self.ansi[idx as usize],
            8..=15 => self.brights[(idx - 8) as usize],
            _ => self.foreground,
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
            key: "f".into(),
            mods: "CTRL".into(),
            timeout_ms: 1000,
        }
    }
}

/// A single snippet entry, parsed from `config.snippets` in Lua.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetConfig {
    /// Display name shown in the command palette.
    pub name: String,
    /// Text written to the PTY when the snippet is expanded.
    pub body: String,
    /// Optional short keyword; typing it then pressing Tab expands the snippet directly.
    pub trigger: Option<String>,
}

/// A single key binding entry, parsed from `config.keys` in Lua.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBind {
    /// Modifier string: "LEADER", "CMD", "CMD|SHIFT", "CTRL|SHIFT", …
    pub mods: String,
    /// The key character or name, e.g. "a", "%", '"'.
    pub key: String,
    /// Action name string, matched against `Action::from_str`.
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    #[serde(skip_serializing)]
    pub api_key: Option<SecretString>,
    pub base_url: Option<String>,
    pub context_lines: u32,
    pub features: LlmFeatures,
    pub ui: ChatUiConfig,
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
            ui: ChatUiConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatUiConfig {
    pub width_cols: u16,
    pub background: [f32; 4],
    pub user_fg: [f32; 4],
    pub assistant_fg: [f32; 4],
    pub input_fg: [f32; 4],
}

impl Default for ChatUiConfig {
    fn default() -> Self {
        Self {
            width_cols: 55,
            background: [0.10, 0.09, 0.16, 1.0],
            user_fg: [0.75, 0.90, 1.00, 1.0],
            assistant_fg: [0.55, 1.00, 0.53, 1.0],
            input_fg: [1.00, 1.00, 1.00, 1.0],
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
