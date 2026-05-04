pub mod lua;
pub mod schema;
pub mod watcher;

pub use schema::Config;

use anyhow::Result;
use std::path::PathBuf;

/// Default config source embedded in the binary.
const DEFAULT_SYSTEM_PROMPT: &str = include_str!("../../config/default/system/system_prompt.md");
const DEFAULT_CONFIG: &str = include_str!("../../config/default/config.lua");
const DEFAULT_UI: &str = include_str!("../../config/default/ui.lua");
const DEFAULT_PERF: &str = include_str!("../../config/default/perf.lua");
const DEFAULT_KEYBINDS: &str = include_str!("../../config/default/keybinds.lua");
const DEFAULT_LLM: &str = include_str!("../../config/default/llm.lua");
const DEFAULT_SNIPPETS: &str = include_str!("../../config/default/snippets.lua");
const DEFAULT_NOTIFICATIONS: &str = include_str!("../../config/default/notifications.lua");
const SHELL_INTEGRATION_ZSH: &str = include_str!("../../scripts/shell-integration.zsh");

// Bundled theme files — seeded into ~/.config/petruterm/themes/ on first launch.
const THEME_DRACULA_PRO: &str = include_str!("../../assets/themes/dracula-pro.lua");
const THEME_TOKYO_NIGHT: &str = include_str!("../../assets/themes/tokyo-night.lua");
const THEME_CATPPUCCIN_MOCHA: &str = include_str!("../../assets/themes/catppuccin-mocha.lua");
const THEME_ONE_DARK: &str = include_str!("../../assets/themes/one-dark.lua");
const THEME_GRUVBOX_DARK: &str = include_str!("../../assets/themes/gruvbox-dark.lua");

/// Modules preloaded for the embedded fallback config (no filesystem access).
pub const EMBEDDED_MODULES: &[(&str, &str)] = &[
    ("ui", DEFAULT_UI),
    ("perf", DEFAULT_PERF),
    ("keybinds", DEFAULT_KEYBINDS),
    ("llm", DEFAULT_LLM),
    ("snippets", DEFAULT_SNIPPETS),
    ("notifications", DEFAULT_NOTIFICATIONS),
];

/// Resolve the user config directory: ~/.config/petruterm/
///
/// Follows XDG: respects $XDG_CONFIG_HOME, falls back to ~/.config.
/// Uses this path on all platforms (including macOS) instead of
/// ~/Library/Application Support/ so the config is shell-accessible.
pub fn config_dir() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".config")
        })
        .join("petruterm")
}

/// Resolve the main config file path: ~/.config/petruterm/config.lua
pub fn config_path() -> PathBuf {
    config_dir().join("config.lua")
}

/// Resolve the themes directory: ~/.config/petruterm/themes/
pub fn themes_dir() -> PathBuf {
    config_dir().join("themes")
}

/// Resolve the system prompt file: ~/.config/petruterm/system/system_prompt.md
pub fn system_prompt_path() -> PathBuf {
    config_dir().join("system").join("system_prompt.md")
}

/// Load the system prompt from disk, falling back to the embedded default.
pub fn load_system_prompt() -> String {
    std::fs::read_to_string(system_prompt_path())
        .unwrap_or_else(|_| DEFAULT_SYSTEM_PROMPT.to_string())
}

/// Scan the themes directory and return a sorted list of theme names (stem of each .lua file).
pub fn list_themes() -> Vec<String> {
    let dir = themes_dir();
    if !dir.exists() {
        return vec![];
    }
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().map(|x| x == "lua").unwrap_or(false) {
                p.file_stem()?.to_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();
    names.sort();
    names
}

/// Load the user config, falling back to the embedded default if the file doesn't exist.
/// Returns both the parsed Config and the live Lua VM (which holds registered event callbacks).
pub fn load() -> Result<(Config, mlua::Lua)> {
    let dir = config_dir();
    let path = config_path();

    if !dir.exists() {
        log::info!("Config dir not found; creating {}", dir.display());
        std::fs::create_dir_all(&dir)?;
    }
    // Always ensure all config files exist (idempotent — skips files already present).
    ensure_default_configs(&dir)?;
    install_shell_integration(&dir)?;

    update_managed_configs(&dir);

    if path.exists() {
        log::info!("Loading config: {}", path.display());
        lua::load_config(&path)
    } else {
        log::warn!("Config file not found; using built-in defaults");
        lua::load_config_str(DEFAULT_CONFIG, "default/config.lua", EMBEDDED_MODULES)
    }
}

/// Reload the config (called by hot-reload watcher).
/// Returns both the parsed Config and a fresh Lua VM with any new callbacks registered.
pub fn reload() -> Result<(Config, mlua::Lua)> {
    let path = config_path();
    if path.exists() {
        lua::load_config(&path)
    } else {
        lua::load_config_str(DEFAULT_CONFIG, "default/config.lua", EMBEDDED_MODULES)
    }
}

/// Auto-update managed config files whose bundled version is newer than the installed one.
///
/// Only files that include a `-- petruterm-config-version: N` line are managed.
/// User-customizable files (ui.lua, perf.lua, llm.lua) are intentionally NOT versioned
/// so this function never overwrites them.
fn update_managed_configs(dir: &std::path::Path) {
    let managed: &[(&str, &str)] = &[("keybinds.lua", DEFAULT_KEYBINDS)];
    for (name, bundled) in managed {
        let dest = dir.join(name);
        let needs_update = if dest.exists() {
            // Read only the first 256 bytes — the version tag is always in the first line
            // so we never read the whole file just to compare a version number (TD-036).
            let bundled_ver = extract_lua_version(bundled);
            let existing_ver = read_first_bytes(&dest, 256)
                .and_then(|s| extract_lua_version(&s).map(|v| v.to_owned()));
            existing_ver.as_deref() != bundled_ver
        } else {
            true
        };
        if needs_update {
            if let Err(e) = std::fs::write(&dest, bundled) {
                log::warn!("Failed to update {name}: {e}");
            } else {
                log::info!("Updated managed config: {}", dest.display());
            }
        }
    }
}

/// Read up to `max_bytes` from a file and return as a String (lossy UTF-8).
fn read_first_bytes(path: &std::path::Path, max_bytes: usize) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; max_bytes];
    let n = f.read(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf[..n]).into_owned())
}

/// Extract the `-- petruterm-config-version: N` tag from a Lua config file.
fn extract_lua_version(content: &str) -> Option<&str> {
    content
        .lines()
        .find(|l| l.trim_start().starts_with("-- petruterm-config-version:"))
        .map(|l| l.trim())
}

/// Write default config files that don't yet exist. Already-present files are left untouched
/// so user customisations are never overwritten. Safe to call on every launch.
fn ensure_default_configs(dir: &std::path::Path) -> Result<()> {
    let files: &[(&str, &str)] = &[
        (
            "config.lua",
            include_str!("../../config/default/config.lua"),
        ),
        ("ui.lua", include_str!("../../config/default/ui.lua")),
        ("perf.lua", include_str!("../../config/default/perf.lua")),
        (
            "keybinds.lua",
            include_str!("../../config/default/keybinds.lua"),
        ),
        ("llm.lua", include_str!("../../config/default/llm.lua")),
        ("snippets.lua", DEFAULT_SNIPPETS),
        ("notifications.lua", DEFAULT_NOTIFICATIONS),
    ];

    for (name, content) in files {
        let dest = dir.join(name);
        if !dest.exists() {
            std::fs::write(&dest, content)?;
            log::info!("Created default config: {}", dest.display());
        }
    }

    // Seed system prompt into ~/.config/petruterm/system/ (never overwrite user edits).
    let system_dir = dir.join("system");
    if !system_dir.exists() {
        std::fs::create_dir_all(&system_dir)?;
    }
    let system_prompt_dest = system_dir.join("system_prompt.md");
    if !system_prompt_dest.exists() {
        std::fs::write(&system_prompt_dest, DEFAULT_SYSTEM_PROMPT)?;
        log::info!(
            "Created default system prompt: {}",
            system_prompt_dest.display()
        );
    }

    // Seed bundled themes into ~/.config/petruterm/themes/ (never overwrite user edits).
    let themes_dir = dir.join("themes");
    if !themes_dir.exists() {
        std::fs::create_dir_all(&themes_dir)?;
        log::info!("Created themes dir: {}", themes_dir.display());
    }
    let bundled_themes: &[(&str, &str)] = &[
        ("dracula-pro.lua", THEME_DRACULA_PRO),
        ("tokyo-night.lua", THEME_TOKYO_NIGHT),
        ("catppuccin-mocha.lua", THEME_CATPPUCCIN_MOCHA),
        ("one-dark.lua", THEME_ONE_DARK),
        ("gruvbox-dark.lua", THEME_GRUVBOX_DARK),
    ];
    for (name, content) in bundled_themes {
        let dest = themes_dir.join(name);
        if !dest.exists() {
            std::fs::write(&dest, content)?;
            log::info!("Seeded theme: {}", dest.display());
        }
    }

    Ok(())
}

/// Write shell-integration.zsh to the config dir if it doesn't exist or is outdated.
/// Uses a version comment at the top of the file to detect when to update.
fn install_shell_integration(dir: &std::path::Path) -> Result<()> {
    let dest = dir.join("shell-integration.zsh");
    let current_version = extract_version(SHELL_INTEGRATION_ZSH);

    let needs_install = if dest.exists() {
        let existing = std::fs::read_to_string(&dest).unwrap_or_default();
        extract_version(&existing) != current_version
    } else {
        true
    };

    if needs_install {
        std::fs::write(&dest, SHELL_INTEGRATION_ZSH)?;
        log::info!("Installed shell integration: {}", dest.display());
    }

    Ok(())
}

/// Extract the `# version: X` comment from a shell script, if present.
fn extract_version(content: &str) -> Option<&str> {
    content
        .lines()
        .find(|l| l.trim_start().starts_with("# version:"))
        .map(|l| l.trim())
}
