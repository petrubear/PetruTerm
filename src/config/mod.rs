pub mod lua;
pub mod schema;
pub mod watcher;

pub use schema::Config;

use anyhow::Result;
use std::path::PathBuf;

/// Default config source embedded in the binary.
const DEFAULT_CONFIG: &str = include_str!("../../config/default/config.lua");

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

/// Load the user config, falling back to the embedded default if the file doesn't exist.
///
/// On first launch, the default config is copied to ~/.config/petruterm/.
pub fn load() -> Result<Config> {
    let dir = config_dir();
    let path = config_path();

    if !dir.exists() {
        log::info!("Config dir not found; creating {}", dir.display());
        std::fs::create_dir_all(&dir)?;
        copy_default_configs(&dir)?;
    }

    if path.exists() {
        log::info!("Loading config: {}", path.display());
        lua::load_config(&path)
    } else {
        log::warn!("Config file not found; using built-in defaults");
        lua::load_config_str(DEFAULT_CONFIG, "default/config.lua")
    }
}

/// Reload the config (called by hot-reload watcher).
pub fn reload() -> Result<Config> {
    let path = config_path();
    if path.exists() {
        lua::load_config(&path)
    } else {
        lua::load_config_str(DEFAULT_CONFIG, "default/config.lua")
    }
}

/// Copy all default config files to the user config directory on first launch.
fn copy_default_configs(dir: &std::path::Path) -> Result<()> {
    let files: &[(&str, &str)] = &[
        ("config.lua",   include_str!("../../config/default/config.lua")),
        ("ui.lua",       include_str!("../../config/default/ui.lua")),
        ("perf.lua",     include_str!("../../config/default/perf.lua")),
        ("keybinds.lua", include_str!("../../config/default/keybinds.lua")),
        ("llm.lua",      include_str!("../../config/default/llm.lua")),
    ];

    for (name, content) in files {
        let dest = dir.join(name);
        std::fs::write(&dest, content)?;
        log::info!("Wrote default config: {}", dest.display());
    }

    Ok(())
}
