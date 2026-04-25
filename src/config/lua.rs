use anyhow::Result;
use dirs;
use mlua::prelude::*;
use mlua::StdLib;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use super::schema::{ColorScheme, Config, TitleBarStyle};

fn parse_hex_linear(s: &str) -> [f32; 4] {
    let s = s.trim_start_matches('#');
    if s.len() < 6 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(0) as f32 / 255.0;
    let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(0) as f32 / 255.0;
    let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(0) as f32 / 255.0;
    let a = if s.len() >= 8 {
        u8::from_str_radix(&s[6..8], 16).unwrap_or(255) as f32 / 255.0
    } else {
        1.0
    };
    [r, g, b, a]
}

/// Stdlib available to user config scripts.
///
/// Includes `os` (for `os.getenv`) and `package` (for `require`).
/// `io`, `debug`, and `load` are excluded to limit the attack surface —
/// user configs don't need arbitrary file I/O or dynamic code loading.
/// Note: `os.execute` is still available here because this is user-controlled
/// config, not third-party plugins. Phase 4 plugins will use a stricter sandbox.
fn config_stdlib() -> StdLib {
    StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::OS | StdLib::PACKAGE
}

/// Load and evaluate a Lua config file, returning a resolved Config.
///
/// Bytecode cache: compiled Lua is stored at `~/.cache/petruterm/lua-bc/{hash}.luac`.
/// The cache is reused when its mtime is >= the source file's mtime.
/// On any error reading or writing the cache the loader silently falls back to
/// compiling from source, so this is always a transparent optimisation.
pub fn load_config(path: &Path) -> Result<(Config, Lua)> {
    evict_stale_lua_cache();
    let lua = Lua::new_with(config_stdlib(), LuaOptions::default())
        .map_err(|e| anyhow::anyhow!("Lua VM init error: {e}"))?;
    inject_petruterm_global(&lua).map_err(|e| anyhow::anyhow!("Lua setup error: {e}"))?;
    inject_require_path(&lua, path).map_err(|e| anyhow::anyhow!("Lua path error: {e}"))?;

    let config_src = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config {}: {e}", path.display()))?;

    let config_table: LuaTable = load_or_compile_config(&lua, path, &config_src)
        .map_err(|e| anyhow::anyhow!("Lua eval error in {}: {e}", path.display()))?;

    let config =
        table_to_config(config_table).map_err(|e| anyhow::anyhow!("Config parse error: {e}"))?;
    Ok((config, lua))
}

/// Compute a stable u64 hash of a path string.
fn hash_path(path: &Path) -> u64 {
    let mut h = DefaultHasher::new();
    path.to_string_lossy().hash(&mut h);
    h.finish()
}

/// Return the path to `~/.cache/petruterm/lua-bc/{version}/{hash}.luac`.
/// The version subdirectory automatically invalidates cache entries from older binaries.
fn bytecode_cache_path(src_path: &Path) -> Option<std::path::PathBuf> {
    let cache_dir = dirs::cache_dir()?
        .join("petruterm")
        .join("lua-bc")
        .join(env!("CARGO_PKG_VERSION"));
    Some(cache_dir.join(format!("{:016x}.luac", hash_path(src_path))))
}

/// Remove stale bytecode cache entries: old version directories and .luac files not
/// modified in more than 30 days. Called once at startup; errors are silently ignored.
fn evict_stale_lua_cache() {
    let Some(cache_root) = dirs::cache_dir().map(|d| d.join("petruterm").join("lua-bc")) else {
        return;
    };
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(30 * 24 * 3600))
        .unwrap_or(std::time::UNIX_EPOCH);
    let current_version = env!("CARGO_PKG_VERSION");
    let Ok(entries) = std::fs::read_dir(&cache_root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if entry.file_name().to_string_lossy() != current_version {
            let _ = std::fs::remove_dir_all(&path);
            continue;
        }
        // Current version dir: evict .luac files not touched in >30 days.
        if let Ok(files) = std::fs::read_dir(&path) {
            for file in files.flatten() {
                let fp = file.path();
                if fp.extension().is_some_and(|e| e == "luac") {
                    let stale = fp
                        .metadata()
                        .and_then(|m| m.modified())
                        .map(|m| m < cutoff)
                        .unwrap_or(false);
                    if stale {
                        let _ = std::fs::remove_file(&fp);
                    }
                }
            }
        }
    }
}

/// Get the mtime of a file as a `SystemTime`, returns `None` on any error.
fn mtime(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// Try to load bytecode from cache; compile from source and cache on miss.
/// On any cache I/O error, silently falls back to fresh compilation.
fn load_or_compile_config(lua: &Lua, src_path: &Path, src: &str) -> LuaResult<LuaTable> {
    // Attempt to use bytecode cache.
    if let Some(cache_path) = bytecode_cache_path(src_path) {
        let src_mtime = mtime(src_path);
        let cache_mtime = mtime(&cache_path);

        // Cache hit: cache exists and is at least as new as the source.
        let use_cache = match (src_mtime, cache_mtime) {
            (Some(sm), Some(cm)) => cm >= sm,
            _ => false,
        };

        if use_cache {
            if let Ok(bytecode) = std::fs::read(&cache_path) {
                match lua
                    .load(&bytecode[..])
                    .set_name("config.lua")
                    .eval::<LuaTable>()
                {
                    Ok(t) => {
                        log::debug!(
                            "Loaded Lua config from bytecode cache: {}",
                            cache_path.display()
                        );
                        return Ok(t);
                    }
                    Err(e) => {
                        log::warn!("Bytecode cache invalid, recompiling: {e}");
                        // Fall through to recompile.
                    }
                }
            }
        }

        // Cache miss or stale — compile from source and store bytecode.
        let func: LuaFunction = lua.load(src).set_name("config.lua").into_function()?;
        // dump(strip=true) removes debug info (line numbers, local names) for smaller cache.
        let bytecode: Vec<u8> = func.dump(true);

        // Evaluate the function to get the config table.
        let config_table: LuaTable = func.call(())?;

        // Write cache asynchronously-ish: ignore errors so a read-only FS never breaks loading.
        if let Some(parent) = cache_path.parent() {
            if std::fs::create_dir_all(parent).is_ok() {
                if let Err(e) = std::fs::write(&cache_path, &bytecode) {
                    log::warn!(
                        "Failed to write Lua bytecode cache {}: {e}",
                        cache_path.display()
                    );
                } else {
                    log::debug!("Cached Lua bytecode: {}", cache_path.display());
                }
            }
        }

        Ok(config_table)
    } else {
        // No cache dir available — compile directly.
        lua.load(src).set_name("config.lua").eval()
    }
}

/// Evaluate a Lua config from embedded source (used for defaults).
///
/// `preloaded` is a list of `(module_name, source)` pairs registered into
/// `package.preload` so that `require("ui")` etc. work without the filesystem.
pub fn load_config_str(src: &str, name: &str, preloaded: &[(&str, &str)]) -> Result<(Config, Lua)> {
    let lua = Lua::new_with(config_stdlib(), LuaOptions::default())
        .map_err(|e| anyhow::anyhow!("Lua VM init error: {e}"))?;
    inject_petruterm_global(&lua).map_err(|e| anyhow::anyhow!("Lua setup error: {e}"))?;
    inject_preloaded_modules(&lua, preloaded)
        .map_err(|e| anyhow::anyhow!("Lua preload error: {e}"))?;

    let config_table: LuaTable = lua
        .load(src)
        .set_name(name)
        .eval()
        .map_err(|e| anyhow::anyhow!("Lua eval error in {name}: {e}"))?;

    let config = table_to_config(config_table)
        .map_err(|e| anyhow::anyhow!("Config parse error in {name}: {e}"))?;
    Ok((config, lua))
}

/// Load a theme file (returns a `ColorScheme` table) from disk.
///
/// Theme files return a Lua table with hex color strings. Example:
/// ```lua
/// return { name="Tokyo Night", foreground="#c0caf5", background="#1a1b26", ... }
/// ```
pub fn load_theme(path: &Path) -> Result<ColorScheme> {
    // Themes are pure data (color tables) — no OS, filesystem, or module access needed.
    let lua = Lua::new_with(StdLib::TABLE | StdLib::STRING, LuaOptions::default())
        .map_err(|e| anyhow::anyhow!("Lua VM init error: {e}"))?;
    let src = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read theme {}: {e}", path.display()))?;
    let table: LuaTable = lua
        .load(&src)
        .set_name(path.to_string_lossy().as_ref())
        .eval()
        .map_err(|e| anyhow::anyhow!("Lua error in theme {}: {e}", path.display()))?;
    table_to_color_scheme(table)
        .map_err(|e| anyhow::anyhow!("Theme parse error in {}: {e}", path.display()))
}

/// Parse a Lua table (hex strings) into a `ColorScheme`.
fn table_to_color_scheme(table: LuaTable) -> LuaResult<ColorScheme> {
    let get_color = |key: &str| -> [f32; 4] {
        table
            .get::<String>(key)
            .map(|s| parse_hex_linear(&s))
            .unwrap_or([0.0, 0.0, 0.0, 1.0])
    };
    let get_palette = |key: &str| -> [[f32; 4]; 8] {
        let mut arr = [[0.0f32; 4]; 8];
        if let Ok(t) = table.get::<LuaTable>(key) {
            for (i, slot) in arr.iter_mut().enumerate() {
                if let Ok(s) = t.get::<String>((i + 1) as i64) {
                    *slot = parse_hex_linear(&s);
                }
            }
        }
        arr
    };
    let get_opt_color = |key: &str| -> [f32; 4] {
        table
            .get::<String>(key)
            .map(|s| parse_hex_linear(&s))
            .unwrap_or([0.0; 4])
    };
    let mut scheme = ColorScheme {
        foreground: get_color("foreground"),
        background: get_color("background"),
        cursor_bg: get_color("cursor_bg"),
        cursor_fg: get_color("cursor_fg"),
        cursor_border: get_color("cursor_border"),
        selection_bg: get_color("selection_bg"),
        selection_fg: get_color("selection_fg"),
        ansi: get_palette("ansi"),
        brights: get_palette("brights"),
        ui_accent: get_opt_color("ui_accent"),
        ui_surface: get_opt_color("ui_surface"),
        ui_surface_active: get_opt_color("ui_surface_active"),
        ui_surface_hover: get_opt_color("ui_surface_hover"),
        ui_muted: get_opt_color("ui_muted"),
        ui_success: get_opt_color("ui_success"),
        ui_overlay: get_opt_color("ui_overlay"),
    };
    scheme.derive_ui_colors();
    Ok(scheme)
}

/// Inject the `petruterm` global table into the Lua VM.
fn inject_petruterm_global(lua: &Lua) -> LuaResult<()> {
    let petruterm = lua.create_table()?;

    // petruterm.font("Family1, Family2, ...") → resolves and returns the first available family.
    // Falls back to the first monospace font found on the system if none match.
    let font_fn = lua.create_function(|_, families_str: String| {
        use crate::font::locator::FontLocator;
        use font_kit::source::SystemSource;

        let locator = FontLocator::new();

        for family in families_str.split(',').map(|s| s.trim()) {
            if !family.is_empty() && locator.locate_font(family).is_some() {
                log::info!("petruterm.font: resolved '{family}'");
                return Ok(family.to_string());
            }
        }

        // None found — try to pick the first monospace family from the system.
        log::warn!(
            "petruterm.font: none of [{}] found on system, scanning for monospace fallback",
            families_str
        );
        let source = SystemSource::new();
        if let Ok(families) = source.all_families() {
            if let Some(fb) = families.into_iter().find(|name| {
                let n = name.to_lowercase();
                n.contains("mono")
                    || n.contains("code")
                    || n.contains("courier")
                    || n.contains("consol")
            }) {
                log::warn!("petruterm.font: using system fallback '{fb}'");
                return Ok(fb);
            }
        }

        // Absolute last resort: return the first entry and let build_font_system error clearly.
        let first = families_str
            .split(',')
            .next()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        log::warn!(
            "petruterm.font: no monospace font found, using '{first}' (may fail at startup)"
        );
        Ok(first)
    })?;
    petruterm.set("font", font_fn)?;

    // petruterm.action — table of action name strings.
    // Each key maps to itself so Lua can write `petruterm.action.NewTab`
    // and get the string "NewTab" that Rust then resolves via Action::from_str.
    let action = lua.create_table()?;
    for name in &[
        "CommandPalette",
        "ToggleAiPanel",
        "ToggleAiMode", // legacy alias kept for compatibility
        "FocusAiPanel",
        "ExplainLastOutput",
        "ToggleStatusBar",
        "FixLastError",
        "UndoLastWrite",
        "SplitHorizontal",
        "SplitVertical",
        "ActivatePane",
        "ClosePane",
        "FocusPaneLeft",
        "FocusPaneRight",
        "FocusPaneUp",
        "FocusPaneDown",
        "NewTab",
        "CloseTab",
        "NextTab",
        "PrevTab",
        "RenameTab",
        "ToggleFullscreen",
        "Quit",
    ] {
        action.set(*name, *name)?;
    }
    petruterm.set("action", action)?;

    // _pt_handlers: { event_name: [fn, ...] } — populated by petruterm.on().
    lua.set_named_registry_value("_pt_handlers", lua.create_table()?)?;

    // petruterm.on(event, fn) — register a callback for a named event.
    let on_fn = lua.create_function(|lua, (event, cb): (String, LuaFunction)| {
        let handlers: LuaTable = lua.named_registry_value("_pt_handlers")?;
        let cbs: LuaTable = match handlers.get::<Option<LuaTable>>(event.as_str())? {
            Some(t) => t,
            None => {
                let t = lua.create_table()?;
                handlers.set(event.as_str(), t.clone())?;
                t
            }
        };
        cbs.push(cb)?;
        Ok(())
    })?;
    petruterm.set("on", on_fn)?;

    // petruterm.notify(msg [, ms]) — queue a toast notification.
    // Stored as two registry values; Rust drains them after each event dispatch.
    let notify_fn = lua.create_function(|lua, (msg, ms): (String, Option<u64>)| {
        lua.set_named_registry_value("_pt_notify_msg", msg)?;
        lua.set_named_registry_value("_pt_notify_ms", ms.unwrap_or(3000))?;
        Ok(())
    })?;
    petruterm.set("notify", notify_fn)?;

    lua.globals().set("petruterm", petruterm)?;

    // Also register as a loadable module so `require('petruterm')` works
    // alongside direct global access.
    lua.load(r#"package.preload['petruterm'] = function() return petruterm end"#)
        .exec()?;

    Ok(())
}

/// Call all Lua callbacks registered for `event` via `petruterm.on()`.
pub fn fire_lua_event(lua: &Lua, event: &str) {
    let Ok(handlers) = lua.named_registry_value::<LuaTable>("_pt_handlers") else {
        return;
    };
    let Ok(cbs) = handlers.get::<LuaTable>(event) else {
        return;
    };
    for f in cbs.sequence_values::<LuaFunction>().flatten() {
        if let Err(e) = f.call::<()>(()) {
            log::warn!("petruterm.on({event}) callback error: {e}");
        }
    }
}

/// Drain a pending `petruterm.notify()` call, returning `(message, duration_ms)` if one exists.
pub fn drain_lua_toast(lua: &Lua) -> Option<(String, u64)> {
    let msg = lua.named_registry_value::<String>("_pt_notify_msg").ok()?;
    let ms = lua
        .named_registry_value::<u64>("_pt_notify_ms")
        .unwrap_or(3000);
    let _ = lua.unset_named_registry_value("_pt_notify_msg");
    let _ = lua.unset_named_registry_value("_pt_notify_ms");
    Some((msg, ms))
}

/// Register embedded Lua sources into `package.preload` so `require()` works
/// when there is no config directory on the filesystem (embedded fallback).
fn inject_preloaded_modules(lua: &Lua, modules: &[(&str, &str)]) -> LuaResult<()> {
    let package: LuaTable = lua.globals().get("package")?;
    let preload: LuaTable = package.get("preload")?;
    for (mod_name, mod_src) in modules {
        let func = lua.load(*mod_src).into_function()?;
        preload.set(*mod_name, func)?;
    }
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
        if let Ok(style) = win.get::<String>("title_bar_style") {
            config.window.title_bar_style = match style.as_str() {
                "none" | "None" => TitleBarStyle::None,
                "native" | "Native" => TitleBarStyle::Native,
                _ => TitleBarStyle::Custom,
            };
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

    if let Ok(leader_table) = table.get::<LuaTable>("leader") {
        if let Ok(k) = leader_table.get::<String>("key") {
            config.leader.key = k;
        }
        if let Ok(m) = leader_table.get::<String>("mods") {
            config.leader.mods = m;
        }
        if let Ok(t) = leader_table.get::<u64>("timeout_ms") {
            config.leader.timeout_ms = t;
        }
    }

    if let Ok(keys_table) = table.get::<LuaTable>("keys") {
        for entry in keys_table.sequence_values::<LuaTable>().flatten() {
            let mods: String = entry.get("mods").unwrap_or_default();
            let key: String = entry.get("key").unwrap_or_default();
            let action: String = entry.get("action").unwrap_or_default();
            if !mods.is_empty() && !key.is_empty() && !action.is_empty() {
                config
                    .keys
                    .push(super::schema::KeyBind { mods, key, action });
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
            config.llm.api_key = Some(k.into());
        }
        if let Ok(u) = llm_table.get::<String>("base_url") {
            config.llm.base_url = Some(u);
        }
        if let Ok(c) = llm_table.get::<u32>("context_lines") {
            config.llm.context_lines = c;
        }

        if let Ok(ui_table) = llm_table.get::<LuaTable>("ui") {
            if let Ok(w) = ui_table.get::<u16>("width_cols") {
                config.llm.ui.width_cols = w;
            }
            if let Ok(bg) = ui_table.get::<String>("background") {
                config.llm.ui.background = parse_hex_linear(&bg);
            }
            if let Ok(ufg) = ui_table.get::<String>("user_fg") {
                config.llm.ui.user_fg = parse_hex_linear(&ufg);
            }
            if let Ok(afg) = ui_table.get::<String>("assistant_fg") {
                config.llm.ui.assistant_fg = parse_hex_linear(&afg);
            }
            if let Ok(ifg) = ui_table.get::<String>("input_fg") {
                config.llm.ui.input_fg = parse_hex_linear(&ifg);
            }
        }
    }

    if let Ok(snippets_table) = table.get::<LuaTable>("snippets") {
        for entry in snippets_table.sequence_values::<LuaTable>().flatten() {
            let name: String = entry.get("name").unwrap_or_default();
            let body: String = entry.get("body").unwrap_or_default();
            let trigger: Option<String> = entry.get("trigger").ok();
            if !name.is_empty() && !body.is_empty() {
                config.snippets.push(super::schema::SnippetConfig {
                    name,
                    body,
                    trigger,
                });
            }
        }
    }

    if let Ok(sb_table) = table.get::<LuaTable>("status_bar") {
        if let Ok(e) = sb_table.get::<bool>("enabled") {
            config.status_bar.enabled = e;
        }
        if let Ok(p) = sb_table.get::<String>("position") {
            config.status_bar.position = match p.as_str() {
                "top" | "Top" => crate::config::schema::StatusBarPosition::Top,
                _ => crate::config::schema::StatusBarPosition::Bottom,
            };
        }
        if let Ok(s) = sb_table.get::<String>("style") {
            config.status_bar.style = match s.as_str() {
                "powerline" | "Powerline" => crate::config::schema::StatusBarStyle::Powerline,
                _ => crate::config::schema::StatusBarStyle::Plain,
            };
        }
        if let Ok(d) = sb_table.get::<bool>("git_dirty_check") {
            config.status_bar.git_dirty_check = d;
        }
    }

    if let Ok(bs) = table.get::<String>("battery_saver") {
        config.battery_saver = match bs.as_str() {
            "always" | "Always" => crate::config::schema::BatterySaverMode::Always,
            "never" | "Never" => crate::config::schema::BatterySaverMode::Never,
            _ => crate::config::schema::BatterySaverMode::Auto,
        };
    }

    if let Ok(gp) = table.get::<String>("gpu_preference") {
        config.gpu_preference = match gp.as_str() {
            "high_performance" | "HighPerformance" => {
                crate::config::schema::GpuPreference::HighPerformance
            }
            "none" | "None" => crate::config::schema::GpuPreference::None,
            _ => crate::config::schema::GpuPreference::LowPower,
        };
    }

    if let Ok(kb_table) = table.get::<LuaTable>("keyboard") {
        if let Ok(v) = kb_table.get::<bool>("option_as_meta") {
            config.keyboard.option_as_meta = v;
        }
    }

    Ok(config)
}
