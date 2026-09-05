// gpui chrome migration (M1b): font identity + real cell metrics, split out
// of `terminal_element.rs` (M1a's font/metrics work was already
// self-contained; this plan's later tasks need `terminal_element.rs` back
// under the project's 400-line module convention before adding more to it).

use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use cosmic_text::{fontdb, Attrs, Buffer, Family, FontSystem, Metrics, Shaping};
use gpui::{px, App, Pixels};

use crate::config::schema::FontConfig;
use crate::font::shaper::FreeTypeCmapLookup;

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
/// entries (see `rasterize::evict_all`'s doc comment for why
/// `cache.clear()` alone would leak GPU memory).
pub fn reload_font_config(font_config: FontConfig, cx: &mut App) {
    let (new_font_system, new_family, face_id, path, face_index) =
        match crate::font::loader::build_font_system(&font_config) {
            Ok(v) => v,
            Err(e) => {
                log::error!("gpui-shell: failed to reload font on config change: {e:#}");
                return;
            }
        };
    let mut font_system = new_font_system;
    // Built before `compute_cell_size`, which now reads its hinted metrics.
    let ft_cmap = FreeTypeCmapLookup::new(&path, face_index, font_config.size);
    if ft_cmap.is_none() {
        log::warn!(
            "gpui-shell: FreeType cmap lookup unavailable after font reload -- Nerd Font PUA icons may not render."
        );
    }
    let cell_size = compute_cell_size(
        &mut font_system,
        &new_family,
        font_config.size,
        font_config.line_height,
        ft_cmap.as_ref(),
    );
    let primary_face_ids = collect_primary_face_ids(&font_system, face_id, &new_family);
    FONT_SYSTEM.with_borrow_mut(|state| {
        state.font_system = font_system;
        state.family = new_family;
        state.size = font_config.size;
        state.line_height = font_config.line_height;
        state.primary_font_id = face_id;
        state.primary_face_ids = primary_face_ids;
        state.ft_cmap = ft_cmap;
    });
    CELL_SIZE.set(cell_size);

    // Evict every cached rasterized frame -- they're keyed on content hash,
    // not font identity, so every one is stale the moment the font changes.
    // This calls back into `rasterize` (not inlined here) because that's
    // where the frame cache lives.
    crate::gpui_shell::rasterize::evict_all(cx);
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
    /// fontdb face ID of the configured primary font. Needed to build a
    /// corrected `CacheKey` when a PUA glyph has to be re-pointed at this
    /// face (see `PuaContext`).
    primary_font_id: fontdb::ID,
    /// Every fontdb face ID belonging to the primary font's family (regular,
    /// bold, italic, bold-italic). A shaped glyph is a true *fallback* only
    /// when its `font_id` is NOT in this set — using the whole family avoids
    /// false-positive PUA overrides when cosmic-text legitimately picks the
    /// bold or italic face of the same font.
    primary_face_ids: HashSet<fontdb::ID>,
    /// Direct FreeType cmap handle on the primary font file. `None` is a
    /// valid outcome (FreeType init/face-open failure) — the PUA override
    /// then simply never fires.
    ft_cmap: Option<FreeTypeCmapLookup>,
}

/// Read-only view of the PUA-correction state, handed to `rasterize` by
/// `with_font_system` so it can fix up misrouted Nerd Font icon glyphs.
///
/// Why this is needed: Nerd Font patches routinely ship malformed or missing
/// OS/2 Unicode-range bits. fontdb derives coverage from those bits, so
/// cosmic-text either reports `glyph_id == 0` for a Private-Use-Area icon
/// codepoint, or silently routes it to some *other* fallback face that has no
/// such icon — even when `Family::Name(primary)` was requested explicitly.
/// `FreeTypeCmapLookup` reads the font's cmap directly (`FT_Get_Char_Index`),
/// bypassing the OS/2 check entirely, which is ground truth. This is the exact
/// same correction `font::shaper` applies for the wgpu renderer (see its
/// `should_override` block); without it the gpui path renders Nerd Font icons
/// from the wrong face, which reads visually as split/fragmented glyphs.
pub(crate) struct PuaContext<'a> {
    pub primary_font_id: fontdb::ID,
    pub primary_face_ids: &'a HashSet<fontdb::ID>,
    pub ft_cmap: Option<&'a FreeTypeCmapLookup>,
}

/// Collect every fontdb face ID sharing `face_id`'s canonical family name —
/// ported from `font::shaper::TextShaper::new`, which builds `primary_face_ids`
/// the same way for the same reason.
fn collect_primary_face_ids(
    font_system: &FontSystem,
    face_id: fontdb::ID,
    actual_family: &str,
) -> HashSet<fontdb::ID> {
    let canonical_family = font_system
        .db()
        .face(face_id)
        .and_then(|f| f.families.first())
        .map(|(name, _)| name.clone())
        .unwrap_or_else(|| actual_family.to_string());

    font_system
        .db()
        .faces()
        .filter(|face| {
            face.families
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(&canonical_family))
        })
        .map(|face| face.id)
        .collect()
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
        let (font_system, family, face_id, path, face_index) =
            crate::font::loader::build_font_system(font_config)
                .expect("load configured font for terminal ligature rendering");
        let primary_face_ids = collect_primary_face_ids(&font_system, face_id, &family);
        let ft_cmap = FreeTypeCmapLookup::new(&path, face_index, font_config.size);
        if ft_cmap.is_none() {
            log::warn!(
                "gpui-shell: FreeType cmap lookup unavailable -- Nerd Font PUA icons may not render."
            );
        }
        FontState {
            font_system,
            family,
            size: font_config.size,
            line_height: font_config.line_height,
            primary_font_id: face_id,
            primary_face_ids,
            ft_cmap,
        }
    });

    // Real cell width/height for the current font/size/line-height, computed
    // once here (and again in `reload_font_config` on a config change) —
    // the single source of truth both `measured_cell_size` (render path) and
    // `gpui_shell::spawn_terminal` (PTY winsize) read, so the two can never
    // disagree.
    static CELL_SIZE: Cell<(Pixels, Pixels)> = Cell::new(FONT_SYSTEM.with_borrow_mut(|state| {
        compute_cell_size(
            &mut state.font_system,
            &state.family,
            state.size,
            state.line_height,
            state.ft_cmap.as_ref(),
        )
    }));
}

/// Real cell width/height for `family` at `size`/`line_height`, mirroring
/// `font::shaper::TextShaper::measure_cell()`'s two-branch structure: prefer
/// FreeType's own hinted metrics, and only fall back to shaping a sample
/// string when FreeType is unavailable.
///
/// The FreeType branch is not an optimization — it is what makes the two
/// renderers agree. FreeType (`FT_LOAD_DEFAULT`) grid-fits each glyph, so its
/// reported advance is the hinted, whole-pixel one the glyphs are actually
/// rasterized against; cosmic-text's shaped advance is the font's unhinted
/// design value. For MonoLisaCode at 16pt those differ (10.0 vs 10.24), and
/// this file previously used only the shaping branch — so every column sat
/// 2.4% further right than the glyph ink drawn into it, leaving a visible
/// sliver of extra space beside every character that the wgpu renderer,
/// reading the hinted value, never had. That is the "large space between
/// characters" a dogfood report flagged as present only in gpui-petruterm.
///
/// Measured at the LOGICAL font size; `rasterize_grid` multiplies by the
/// window's scale factor at paint time. `cell_metrics` therefore returns
/// unrounded values (see its own doc comment) — rounding in logical space
/// then doubling on a 2x display cannot reproduce the wgpu renderer's
/// physical-space rounding.
fn compute_cell_size(
    font_system: &mut FontSystem,
    family: &str,
    size: f32,
    line_height: f32,
    ft_cmap: Option<&FreeTypeCmapLookup>,
) -> (Pixels, Pixels) {
    if let Some((width, height)) = ft_cmap.and_then(|ft| ft.cell_metrics()) {
        if width > 0.0 {
            let cell_height = height.max(size * line_height);
            log::info!(
                "gpui-shell: cell size from FreeType: {width:.2}x{cell_height:.2}px (font: '{family}' {size}pt, line_height={line_height})"
            );
            return (px(width), px(cell_height));
        }
    }

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

    // Parity with `font::shaper::TextShaper::measure_cell`'s own
    // `log::info!("Cell size: ...")` line -- gpui_shell had no equivalent,
    // making it impossible to compare the two binaries' computed cell
    // geometry for the same font/config without attaching a debugger. Added
    // while investigating a dogfood report of wide inter-character spacing
    // present only in gpui-petruterm, not the original wgpu petruterm, for
    // the identical font.
    log::info!(
        "gpui-shell: cell size {cell_width:.2}x{cell_height:.2}px (font: '{family}' {size}pt, line_height={line_height})"
    );

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

/// The resolved internal family name the terminal grid actually renders
/// with (from fontdb, per `build_font_system`'s doc comment -- may differ
/// from the config string). Chrome text drawn via gpui's own `div()`/text
/// layout (tab bar, status bar) doesn't automatically pick this up: gpui
/// has no ambient "use the terminal's font" default, so without explicitly
/// applying this, that text silently renders in gpui's own default UI font
/// instead -- visually a different typeface from the terminal grid right
/// next to it.
pub fn font_family() -> String {
    FONT_SYSTEM.with_borrow(|state| state.family.clone())
}

/// Run `f` with mutable access to the current `FontSystem`, an immutable
/// borrow of the resolved font family name, and the PUA-correction context.
/// Used by `rasterize` (cosmic-text's `Buffer` needs `&mut FontSystem` to
/// shape text, and the glyph-draw pass needs `PuaContext` to fix up misrouted
/// Nerd Font icons).
///
/// All three come from one `FONT_SYSTEM` borrow deliberately: they live in the
/// same `RefCell`, so handing them out via two separate accessors would panic
/// the moment `rasterize` nested them.
pub(crate) fn with_font_system<R>(f: impl FnOnce(&mut FontSystem, &str, PuaContext<'_>) -> R) -> R {
    FONT_SYSTEM.with_borrow_mut(|state| {
        let pua = PuaContext {
            primary_font_id: state.primary_font_id,
            primary_face_ids: &state.primary_face_ids,
            ft_cmap: state.ft_cmap.as_ref(),
        };
        f(&mut state.font_system, &state.family, pua)
    })
}
