use anyhow::Result;
use mlua::prelude::*;
use std::path::Path;

use super::schema::Config;

/// Load and evaluate a Lua config file, returning a resolved Config.
pub fn load_config(path: &Path) -> Result<Config> {
    let lua = Lua::new();
    inject_petruterm_global(&lua).map_err(|e| anyhow::anyhow!("Lua setup error: {e}"))?;
    inject_require_path(&lua, path).map_err(|e| anyhow::anyhow!("Lua path error: {e}"))?;

    let config_src = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config {}: {e}", path.display()))?;

    let config_table: LuaTable = lua
        .load(&config_src)
        .set_name("config.lua")
        .eval()
        .map_err(|e| anyhow::anyhow!("Lua eval error in {}: {e}", path.display()))?;

    table_to_config(config_table).map_err(|e| anyhow::anyhow!("Config parse error: {e}"))
}

/// Evaluate a Lua config from embedded source (used for defaults).
pub fn load_config_str(src: &str, name: &str) -> Result<Config> {
    let lua = Lua::new();
    inject_petruterm_global(&lua).map_err(|e| anyhow::anyhow!("Lua setup error: {e}"))?;

    let config_table: LuaTable = lua
        .load(src)
        .set_name(name)
        .eval()
        .map_err(|e| anyhow::anyhow!("Lua eval error in {name}: {e}"))?;

    table_to_config(config_table).map_err(|e| anyhow::anyhow!("Config parse error in {name}: {e}"))
}

/// Inject the `petruterm` global table into the Lua VM.
fn inject_petruterm_global(lua: &Lua) -> LuaResult<()> {
    let petruterm = lua.create_table()?;

    // petruterm.font("Family Name") → returns a font descriptor string
    let font_fn = lua.create_function(|_, family: String| Ok(family))?;
    petruterm.set("font", font_fn)?;

    // petruterm.action — table of action name strings
    let action = lua.create_table()?;
    for name in &[
        "CommandPalette",
        "ToggleAiMode",
        "ExplainOutput",
        "FixLastError",
        "SplitHorizontal",
        "SplitVertical",
        "ActivatePane",
        "ClosePane",
        "NewTab",
        "CloseTab",
        "ToggleFullscreen",
    ] {
        action.set(*name, *name)?;
    }
    petruterm.set("action", action)?;

    // petruterm.on(event, fn) — event registration (no-op for now; Phase 3)
    let on_fn = lua.create_function(|_, (_event, _cb): (String, LuaFunction)| Ok(()))?;
    petruterm.set("on", on_fn)?;

    lua.globals().set("petruterm", petruterm)?;

    // Also register as a loadable module so `require('petruterm')` works
    // alongside direct global access.
    lua.load(r#"package.preload['petruterm'] = function() return petruterm end"#)
        .exec()?;

    Ok(())
}

/// Add the config file's directory to `package.path` so `require("ui")` works.
fn inject_require_path(lua: &Lua, config_path: &Path) -> LuaResult<()> {
    if let Some(dir) = config_path.parent() {
        let dir_str = dir.to_string_lossy();
        let package: LuaTable = lua.globals().get("package")?;
        let existing_path: String = package.get("path")?;
        package.set("path", format!("{dir_str}/?.lua;{existing_path}"))?;
    }
    Ok(())
}

/// Deserialize a Lua config table into our typed Config struct.
///
/// We pick out keys we understand and leave unknown keys alone so user
/// config can include extra fields without breaking anything.
fn table_to_config(table: LuaTable) -> LuaResult<Config> {
    let mut config = Config::default();

    if let Ok(font) = table.get::<LuaTable>("font") {
        if let Ok(family) = font.get::<String>("family") {
            config.font.family = family;
        }
        if let Ok(size) = font.get::<f32>("size") {
            config.font.size = size;
        }
        if let Ok(lh) = font.get::<f32>("line_height") {
            config.font.line_height = lh;
        }
    } else if let Ok(family) = table.get::<String>("font") {
        config.font.family = family;
    }

    if let Ok(size) = table.get::<f32>("font_size") {
        config.font.size = size;
    }

    if let Ok(lh) = table.get::<f32>("font_line_height") {
        config.font.line_height = lh;
    }

    if let Ok(lcd) = table.get::<bool>("lcd_antialiasing") {
        config.font.lcd_antialiasing = lcd;
    }

    if let Ok(features) = table.get::<LuaTable>("font_features") {
        let mut fs = Vec::new();
        for pair in features.sequence_values::<String>() {
            fs.push(pair?);
        }
        config.font.features = fs;
    }

    if let Ok(lines) = table.get::<u32>("scrollback_lines") {
        config.scrollback_lines = lines;
    }

    if let Ok(scroll) = table.get::<bool>("enable_scroll_bar") {
        config.enable_scroll_bar = scroll;
    }

    if let Ok(fps) = table.get::<u32>("max_fps") {
        config.max_fps = fps;
    }

    if let Ok(shell) = table.get::<String>("shell") {
        config.shell = shell;
    }

    if let Ok(si) = table.get::<bool>("shell_integration") {
        config.shell_integration = si;
    }

    if let Ok(win) = table.get::<LuaTable>("window") {
        if let Ok(b) = win.get::<bool>("borderless") {
            config.window.borderless = b;
        }
        if let Ok(m) = win.get::<bool>("start_maximized") {
            config.window.start_maximized = m;
        }
        if let Ok(o) = win.get::<f32>("opacity") {
            config.window.opacity = o;
        }
        if let Ok(w) = win.get::<u32>("initial_width") {
            config.window.initial_width = Some(w);
        }
        if let Ok(h) = win.get::<u32>("initial_height") {
            config.window.initial_height = Some(h);
        }
        if let Ok(pad) = win.get::<LuaTable>("padding") {
            if let Ok(l) = pad.get::<u32>("left") {
                config.window.padding.left = l;
            }
            if let Ok(r) = pad.get::<u32>("right") {
                config.window.padding.right = r;
            }
            if let Ok(t) = pad.get::<u32>("top") {
                config.window.padding.top = t;
            }
            if let Ok(b) = pad.get::<u32>("bottom") {
                config.window.padding.bottom = b;
            }
        }
    }

    if let Ok(llm_table) = table.get::<LuaTable>("llm") {
        if let Ok(e) = llm_table.get::<bool>("enabled") {
            config.llm.enabled = e;
        }
        if let Ok(p) = llm_table.get::<String>("provider") {
            config.llm.provider = p;
        }
        if let Ok(m) = llm_table.get::<String>("model") {
            config.llm.model = m;
        }
        if let Ok(k) = llm_table.get::<String>("api_key") {
            config.llm.api_key = Some(k);
        }
        if let Ok(u) = llm_table.get::<String>("base_url") {
            config.llm.base_url = Some(u);
        }
        if let Ok(c) = llm_table.get::<u32>("context_lines") {
            config.llm.context_lines = c;
        }
    }

    Ok(config)
}
