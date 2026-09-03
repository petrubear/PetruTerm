// gpui chrome migration (M1b): font identity + real cell metrics, split out
// of `terminal_element.rs` (M1a's font/metrics work was already
// self-contained; this plan's later tasks need `terminal_element.rs` back
// under the project's 400-line module convention before adding more to it).

use std::cell::{Cell, RefCell};

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};
use gpui::{px, App, Pixels};

use crate::config::schema::FontConfig;

/// Set once from `main()`, before any window or `TerminalGridElement` exists.
/// `FONT_SYSTEM`'s thread-local initializer reads from this instead of a
/// hardcoded literal — see `set_font_config`.
static FONT_CONFIG: std::sync::OnceLock<FontConfig> = std::sync::OnceLock::new();

/// Must be called exactly once, before the first paint (i.e. before
/// `cx.open_window(...)` in `main()`). Panics if called twice.
pub fn set_font_config(font_config: FontConfig) {
    FONT_CONFIG
        .set(font_config)
        .expect("set_font_config called more than once");
}

/// Rebuild `FONT_SYSTEM`'s cached font/family/size/line-height for a new
/// config, on hot-reload. Unlike `set_font_config` (set-once, for startup),
/// this may be called repeatedly. Also recomputes `CELL_SIZE` (font size and
/// line height feed cell geometry, so a reload that changes either must
/// recompute it too — this is the single source of truth `measured_cell_size`
/// and `gpui_shell::spawn_terminal` both read).
///
/// Takes `&mut App` to explicitly free the outgoing frames' GPU sprite-atlas
/// entries (see `terminal_element::evict_all_frames`'s doc comment for why
/// `cache.clear()` alone would leak GPU memory).
pub fn reload_font_config(font_config: FontConfig, cx: &mut App) {
    let (new_font_system, new_family, _face_id, _path, _face_index) =
        match crate::font::loader::build_font_system(&font_config) {
            Ok(v) => v,
            Err(e) => {
                log::error!("gpui-shell: failed to reload font on config change: {e:#}");
                return;
            }
        };
    let mut font_system = new_font_system;
    let cell_size = compute_cell_size(
        &mut font_system,
        &new_family,
        font_config.size,
        font_config.line_height,
    );
    FONT_SYSTEM.with_borrow_mut(|state| {
        state.font_system = font_system;
        state.family = new_family;
        state.size = font_config.size;
        state.line_height = font_config.line_height;
    });
    CELL_SIZE.set(cell_size);

    // Evict every cached rasterized frame -- they're keyed on content hash,
    // not font identity, so every one is stale the moment the font changes.
    // This calls back into `terminal_element` (not inlined here) because
    // that's where the frame cache lives; Task 2 of this plan moves both the
    // cache and this call's target into a new `rasterize` module, updating
    // this one call site as part of that move.
    crate::gpui_shell::terminal_element::evict_all_frames(cx);
}

/// `FontSystem` + the real config values that feed shaping/metrics, cached
/// together so a config reload (`reload_font_config`) has one place to
/// update all of them consistently.
struct FontState {
    font_system: FontSystem,
    family: String,
    /// Font size in points — real `config.font.size`, not a hardcoded
    /// constant.
    size: f32,
    /// Line height multiplier — real `config.font.line_height`.
    line_height: f32,
}

// `FontSystem::new()` only does a bare system-font scan — it will NOT find
// MonoLisaCode Nerd Font unless that exact family is fully OS-registered,
// and will silently fall back to some default monospace with no ligature
// GSUB rules (renders fine, never ligates, regardless of feature flags).
// The existing wgpu renderer never hits this: `font::loader::build_font_system`
// resolves the configured family via `FontLocator` and explicitly
// `db.load_font_file(...)`s it, then reports the font's ACTUAL internal
// family name from fontdb (which can differ from the config string) — reuse
// that exact, proven path instead of guessing a bare `Family::Name(...)`.
thread_local! {
    static FONT_SYSTEM: RefCell<FontState> = RefCell::new({
        let font_config = FONT_CONFIG
            .get()
            .expect("set_font_config must be called before the first paint");
        let (font_system, family, _face_id, _path, _face_index) =
            crate::font::loader::build_font_system(font_config)
                .expect("load configured font for terminal ligature rendering");
        FontState {
            font_system,
            family,
            size: font_config.size,
            line_height: font_config.line_height,
        }
    });

    // Real cell width/height for the current font/size/line-height, computed
    // once here (and again in `reload_font_config` on a config change) —
    // the single source of truth both `measured_cell_size` (render path) and
    // `gpui_shell::spawn_terminal` (PTY winsize) read, so the two can never
    // disagree.
    static CELL_SIZE: Cell<(Pixels, Pixels)> = Cell::new(FONT_SYSTEM.with_borrow_mut(|state| {
        compute_cell_size(&mut state.font_system, &state.family, state.size, state.line_height)
    }));
}

/// Shape a sample string and read its advance width to get the real cell
/// width/height for `family` at `size`/`line_height` — the same technique
/// `font::shaper::TextShaper::measure_cell()`'s fallback branch uses, ported
/// here rather than importing that (wgpu-atlas-coupled) type.
fn compute_cell_size(
    font_system: &mut FontSystem,
    family: &str,
    size: f32,
    line_height: f32,
) -> (Pixels, Pixels) {
    let metrics = Metrics::new(size, size * line_height);
    let mut buffer = Buffer::new(font_system, metrics);
    let mut buffer = buffer.borrow_with(font_system);
    buffer.set_size(Some(1000.0), Some(1000.0));

    let attrs = Attrs::new().family(Family::Name(family));
    // 16 `M`s, matching TextShaper::measure_cell's own sample — wide
    // enough for a stable average, short enough to stay off any line-wrap
    // boundary at this buffer width.
    buffer.set_text("MMMMMMMMMMMMMMMM", &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(true);

    let run_width = buffer
        .layout_runs()
        .next()
        .map(|run| run.line_w)
        .unwrap_or(size * 0.6 * 16.0);
    let cell_width = (run_width / 16.0).max(1.0);
    let cell_height = metrics.line_height.max(1.0);

    (px(cell_width), px(cell_height))
}

/// Real cell width/height for the current config, cached in `CELL_SIZE` —
/// the same value `gpui_shell::spawn_terminal` uses for the PTY winsize, so
/// the painted grid and the PTY's advertised cell geometry never disagree.
pub fn measured_cell_size() -> (Pixels, Pixels) {
    CELL_SIZE.get()
}

/// Real, current font size in points — replaces the M0-era hardcoded
/// `FONT_SIZE` constant everywhere it was read.
pub fn font_size() -> f32 {
    FONT_SYSTEM.with_borrow(|state| state.size)
}

/// Run `f` with mutable access to the current `FontSystem` and an immutable
/// borrow of the resolved font family name. Used by `rasterize` (cosmic-text's
/// `Buffer` needs `&mut FontSystem` to shape text).
pub fn with_font_system<R>(f: impl FnOnce(&mut FontSystem, &str) -> R) -> R {
    FONT_SYSTEM.with_borrow_mut(|state| f(&mut state.font_system, &state.family))
}
