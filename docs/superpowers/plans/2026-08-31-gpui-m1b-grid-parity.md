# M1b (Grid Parity) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the gpui terminal grid to visual/interactive parity with the existing wgpu app: ANSI colors, cursor shapes + blink, click-drag selection + copy + mouse-report passthrough, and scroll-wheel scrollback + a visual scrollbar.

**Architecture:** Every piece ports proven logic already in the codebase (`src/app/mux/mod.rs`'s color/inverse/selection resolution, `src/term/color.rs`'s `resolve_color`, `src/app/renderer/terminal.rs`'s cursor geometry, `src/app/input/mod.rs`'s click-count/mouse-report, `src/app/renderer/overlay.rs`'s scrollbar geometry) into gpui-native code — this is a wiring job, not new terminal-emulation logic. One architectural decision threads through cursor focus-state, selection, and scrollbar alike: each `TerminalGridElement` owns its own mouse handling via `window.on_mouse_event`, scoped to its own `paint()` bounds. `terminal_element.rs` (457 lines pre-plan, over the 400-line convention) splits into `font_state.rs` (font/metrics state), `rasterize.rs` (color resolution + cosmic-text rasterization), and `mouse.rs` (click/drag/scrollbar-drag/mouse-report), leaving `terminal_element.rs` as orchestration.

**Tech Stack:** gpui 0.2.2 (exact-pinned), cosmic-text 0.18.2, alacritty_terminal 0.25 (grid/selection, untouched), existing `term::color`/`font::shaper::CellStyle` (shared, PTY-agnostic).

**Spec:** `docs/superpowers/specs/2026-08-30-gpui-chrome-migration-design.md`, `### M1b — Grid Parity: Design` section.

## Global Constraints

- `scripts/ci-local.sh` (`cargo clippy --all-features -- -D warnings`, `cargo fmt --check`, `cargo test --lib`, `cargo audit`) is the real CI gate — not the narrower `cargo build`/`cargo test`.
- Stable-only dependencies, exact-pinned. No new dependencies are needed for this plan (everything ports existing crates: gpui, cosmic-text, alacritty_terminal, already in Cargo.toml).
- Must not break the existing `petruterm` (wgpu) binary — the two binaries share the library crate.
- `gpui_shell` must never import winit APIs (a winit `EventLoop` conflicts fatally with gpui owning the macOS `NSApplication` delegate — see `src/gpui_shell/mod.rs`'s `spawn_terminal` doc comment).
- No GPU/rendering/mouse-pixel-math test harness — dogfood only for those. Unit tests only for pure logic: click-count→`SelectionType` mapping, mouse-report escape-sequence byte formatting, scrollbar thumb geometry, selection/inverse-video fg/bg-swap logic.
- Module files stay under 400 lines; this plan's own Task 1 exists specifically to bring `terminal_element.rs` back under that limit before adding more to it.
- Every dogfood step stops and asks the user to confirm before the task's commit.

---

### Task 1: Split `font_state.rs` out of `terminal_element.rs`

Pure mechanical refactor — zero behavior change. Moves the font-identity/metrics concern (which M1a already made self-contained) into its own file, so later tasks in this plan touch a smaller, focused `terminal_element.rs` instead of piling onto an already-over-limit file.

**Files:**
- Create: `src/gpui_shell/font_state.rs`
- Modify: `src/gpui_shell/terminal_element.rs:1-212` (remove the moved block, add a `use` for the new module)
- Modify: `src/gpui_shell/mod.rs:9-10`, `:60`, `:141` (wait — line 141 is `spawn_terminal`'s call to `measured_cell_size`, see below), `:177`, `:257`
- Modify: `src/bin/gpui_petruterm.rs:4`, `:13`

**Interfaces:**
- Produces: `font_state::set_font_config(font_config: FontConfig)`, `font_state::reload_font_config(font_config: FontConfig, cx: &mut App)`, `font_state::measured_cell_size() -> (Pixels, Pixels)`, `font_state::font_size() -> f32`, `font_state::with_font_system<R>(f: impl FnOnce(&mut cosmic_text::FontSystem, &str) -> R) -> R`.
- Consumes: nothing new — this task only relocates existing M1a code.

- [ ] **Step 1: Create `src/gpui_shell/font_state.rs`**

```rust
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
```

- [ ] **Step 2: Modify `src/gpui_shell/terminal_element.rs`**

Remove lines 12-212 (from the top `use` block through `const TEXT_COLOR`'s preceding `measured_cell_size` function — i.e. everything from `use std::cell::{Cell, RefCell};` through the end of `pub fn measured_cell_size()`) and replace with:

```rust
use std::rc::Rc;
use std::sync::Arc;

use cosmic_text::{Color as CosmicColor, FeatureTag, FontFeatures, Metrics, Shaping, SwashCache, Wrap};
use gpui::{
    fill, point, px, size, App, Bounds, Corners, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, RenderImage, Style, Window,
};
use image::{Frame, RgbaImage};

use crate::term::Terminal;
use font_state::{font_size, measured_cell_size, with_font_system};

use super::font_state;
```

(This is a placeholder import list reflecting what remains in `terminal_element.rs` after this task alone — `Attrs`, `Buffer`, `Family`, `FontSystem`, `HashMap`, `Hash`/`Hasher` become unused once this task's block is removed, since `SWASH_CACHE`/`LAST_IMAGE`/`CachedFrame`/the rasterization loop still live here until Task 2 moves them. Run Step 3 below and let the compiler's unused-import warnings under `-D warnings` tell you exactly which of the above to drop — do not guess; fix based on real compiler output.)

Then in `paint()`, replace the two font-state accesses:
- `let font_size = FONT_SYSTEM.with_borrow(|state| state.size);` → `let font_size = font_size();`
- The `FONT_SYSTEM.with_borrow_mut(|state| { let font_system = &mut state.font_system; let actual_family = &state.family; SWASH_CACHE.with_borrow_mut(|swash_cache| { ... }); });` block → `with_font_system(|font_system, actual_family| { SWASH_CACHE.with_borrow_mut(|swash_cache| { ... }); });` (same inner body, just the outer closure call changes).

Add a new function, `evict_all_frames`, taking over the drain-and-drop logic that used to live inline inside the old (now-removed) `reload_font_config` — `font_state::reload_font_config` (Step 1) calls back into this, since `LAST_IMAGE` still lives here in `terminal_element.rs` after this task (Task 2 moves it, and this function, into `rasterize.rs`, updating `font_state.rs`'s one call site accordingly):

```rust
/// Evict every cached rasterized frame's GPU sprite-atlas entry — called by
/// `font_state::reload_font_config` when the font changes, since every
/// cached frame is stale the moment the font changes (keyed on content
/// hash, not font identity). `cache.clear()` alone would free only the
/// Rust-side `Arc<RenderImage>` handles while leaking the underlying GPU
/// texture — gpui's atlas is insert-only (see `LAST_IMAGE`'s doc comment),
/// the exact bug class M0 fixed for the steady-state repaint path.
pub(crate) fn evict_all_frames(cx: &mut App) {
    let evicted: Vec<Arc<RenderImage>> =
        LAST_IMAGE.with_borrow_mut(|cache| cache.drain().map(|(_, frame)| frame.image).collect());
    for image in evicted {
        cx.drop_image(image, None);
    }
}
```

(Place it anywhere at module scope in `terminal_element.rs` — near `SWASH_CACHE`/`LAST_IMAGE`'s `thread_local!` block, which is unchanged and stays in this file until Task 2, is the natural spot.)

- [ ] **Step 3: Build, fix import errors**

```bash
cargo build --bin gpui-petruterm 2>&1
```

Fix any remaining unused-import or unresolved-name errors by adjusting the `use` list from Step 2 — the compiler's own errors are authoritative here, not the placeholder list above.

- [ ] **Step 4: Add `mod font_state;` and update call sites in `src/gpui_shell/mod.rs`**

```rust
mod font_state;
mod key_map;
pub mod terminal_element;
```

(insert `mod font_state;` before the existing `mod key_map;` line at the top of the file — but see Step 6: it must be `pub mod font_state;`, not `mod font_state;`, because `main()` in a separate binary crate needs to call `font_state::set_font_config`.)

Update the three call sites:
- Line 60 (inside `spawn_terminal`): `terminal_element::measured_cell_size()` → `font_state::measured_cell_size()`
- Line 177 (inside the poll loop's config-reload branch): `terminal_element::reload_font_config(font_config, cx)` → `font_state::reload_font_config(font_config, cx)`
- Line 257 (inside `render()`'s `.children(...)` closure): `terminal_element::measured_cell_size()` → `font_state::measured_cell_size()`

- [ ] **Step 5: Build, fix errors**

```bash
cargo build --bin gpui-petruterm 2>&1
```

- [ ] **Step 6: Update `src/bin/gpui_petruterm.rs`**

```rust
use gpui::{
    prelude::*, px, size, App, Application, Bounds, KeyBinding, WindowBounds, WindowOptions,
};
use petruterm::gpui_shell::{font_state, spawn_config_watcher, GpuiShellRoot, SplitDemo};

fn main() {
    // Real user config (~/.config/petruterm/config.lua, falling back to the
    // embedded default) — the same function the wgpu app uses at startup.
    // This replaces M0's hardcoded `Config::default()` + inline font override.
    let (config, _lua) = petruterm::config::load().expect("load config for gpui-petruterm");
    font_state::set_font_config(config.font.clone());

    // Startup-once, like `set_font_config` above — not per-window. See
    // `spawn_config_watcher`'s doc comment for why calling it more than once
    // would race two watcher threads over one process-global slot.
    spawn_config_watcher();

    Application::new().run(move |cx: &mut App| {
        cx.bind_keys([KeyBinding::new("ctrl-f %", SplitDemo, None)]);

        let bounds = Bounds::centered(None, size(px(900.0), px(600.0)), cx);
        let config = config.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |_, cx| cx.new(move |cx| GpuiShellRoot::new(cx, config)),
        )
        .unwrap();
        cx.activate(true);
    });
}
```

Change `mod font_state;` in `src/gpui_shell/mod.rs` (Step 4) to `pub mod font_state;`.

- [ ] **Step 7: Verify the full gate**

```bash
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```

Test count must be unchanged from before this task (this is a pure move, no new tests, no behavior change).

- [ ] **Step 8: Dogfood checkpoint — STOP and ask the user to confirm**

Ask the user to run `cargo run --bin gpui-petruterm`, type some text, confirm ligatures still render (`-> == != >=`) and nothing looks different from before this task — this is a pure refactor, zero visible change is the expected, correct outcome.

- [ ] **Step 9: Commit**

```bash
git add src/gpui_shell/font_state.rs src/gpui_shell/terminal_element.rs src/gpui_shell/mod.rs src/bin/gpui_petruterm.rs
git commit -m "[gpui-migration] refactor: Split font_state.rs out of terminal_element.rs (M1b).

Pure mechanical move, zero behavior change: font identity/metrics
state (FontState, FONT_SYSTEM, CELL_SIZE, compute_cell_size,
set_font_config, reload_font_config, measured_cell_size) moves from
terminal_element.rs into its own font_state.rs. terminal_element.rs
was already at 457 lines (over the project's 400-line convention)
before this plan; this brings it back under the limit before this
plan's later tasks add ANSI colors, cursor shapes, selection
rendering, and a scrollbar.

New accessors font_size()/with_font_system() let terminal_element.rs
(and later, rasterize.rs) reach the font state without terminal_element.rs
owning the thread-locals directly.

scripts/ci-local.sh clean, test count unchanged. Dogfooded: no visible
change (expected for a pure refactor)."
```

---

### Task 2: ANSI colors (fg/bg + bold/italic) + `rasterize.rs` extraction

The gpui grid currently renders every cell in one fixed foreground/background — `cell.fg`/`cell.bg` are never read. This task fixes that and, since it touches the exact code block that needs rewriting anyway, extracts rasterization into its own file at the same time (avoiding a separate refactor-only pass over the same lines). Selection-highlight rendering (fg/bg swap for selected cells) is also implemented here, even though nothing creates a selection yet — `Terminal::start_selection`/`update_selection` already exist (pre-M1b infrastructure) and `term.selection` is free to read from the same `with_term` closure this task is already inside. Task 4 (selection input) will then need zero rendering changes — it only wires mouse input to the `start_selection`/`update_selection` calls that already exist.

**Files:**
- Create: `src/gpui_shell/rasterize.rs`
- Modify: `src/gpui_shell/terminal_element.rs` (remove `SWASH_CACHE`/`LAST_IMAGE`/`CachedFrame`/`evict_all_frames` (added in Task 1) and the whole rasterization block from `paint()`, replace with a call into `rasterize::rasterize_grid`)
- Modify: `src/gpui_shell/font_state.rs` (one-line call-site update: `reload_font_config`'s eviction call moves from `terminal_element::evict_all_frames` to `rasterize::evict_all`)
- Test: `src/gpui_shell/rasterize.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `font_state::with_font_system`, `font_state::font_size` (Task 1).
- Produces: `rasterize::rasterize_grid(terminal: &Rc<Terminal>, cell_width: Pixels, cell_height: Pixels, scale: f32, colors: &ColorScheme, window: &mut Window) -> Option<Arc<RenderImage>>` (used by `terminal_element.rs`'s `paint()`, and later tasks don't need to touch it directly), `rasterize::evict_all(cx: &mut App)` (this task's `evict_all` takes over from Task 1's `terminal_element::evict_all_frames`, which this task deletes — see Step 4).
- Consumes (pure, existing, unmodified): `term::color::resolve_color(color: AnsiColor, scheme: &ColorScheme) -> [f32; 4]`, `font::shaper::CellStyle { bold: bool, italic: bool }`, `alacritty_terminal::term::cell::Flags`, `alacritty_terminal::selection::SelectionRange`.

- [ ] **Step 1: Write the failing test for the pure color-resolution logic**

Create `src/gpui_shell/rasterize.rs` with just the pure function and its test first (TDD for the one piece of real business logic in this task):

```rust
// gpui chrome migration (M1b): ANSI color resolution + cosmic-text
// rasterization of the terminal grid into an RGBA bitmap, plus the GPU
// sprite-atlas frame cache that avoids re-rasterizing/re-uploading when the
// grid hasn't changed. Split out of `terminal_element.rs` (M1a's own
// whole-branch review flagged the file as over the project's 400-line
// convention; this is the single biggest addition this plan makes to it).

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::Color as AnsiColor;
use cosmic_text::{Attrs, Buffer, Color as CosmicColor, Family, FeatureTag, FontFeatures, Shaping, Style, SwashCache, Weight, Wrap};
use gpui::{px, App, Pixels, RenderImage, Window};
use image::{Frame, RgbaImage};

use crate::config::schema::ColorScheme;
use crate::font::shaper::CellStyle;
use crate::term::Terminal;

/// Resolve one cell's (fg, bg) into real theme colors, applying inverse-video
/// and selection-highlight swaps in that order — ported from
/// `src/app/mux/mod.rs`'s row-building loop, which already solves exactly
/// this for the wgpu renderer.
fn resolve_cell_colors(
    fg: AnsiColor,
    bg: AnsiColor,
    flags: Flags,
    in_selection: bool,
    scheme: &ColorScheme,
) -> ([f32; 4], [f32; 4]) {
    let (fg, bg) = if flags.contains(Flags::INVERSE) {
        (bg, fg)
    } else {
        (fg, bg)
    };
    let (fg, bg) = (
        crate::term::color::resolve_color(fg, scheme),
        crate::term::color::resolve_color(bg, scheme),
    );
    if in_selection {
        (bg, fg)
    } else {
        (fg, bg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::NamedColor;

    fn test_scheme() -> ColorScheme {
        let mut scheme = ColorScheme {
            foreground: [1.0, 1.0, 1.0, 1.0],
            background: [0.0, 0.0, 0.0, 1.0],
            cursor_bg: [1.0, 1.0, 1.0, 1.0],
            cursor_fg: [0.0, 0.0, 0.0, 1.0],
            cursor_border: [1.0, 1.0, 1.0, 1.0],
            selection_bg: [0.5, 0.5, 0.5, 1.0],
            selection_fg: [1.0, 1.0, 1.0, 1.0],
            ansi: [[0.0; 4]; 8],
            brights: [[0.0; 4]; 8],
            ui_accent: [0.0; 4],
            ui_surface: [0.0; 4],
            ui_surface_active: [0.0; 4],
            ui_surface_hover: [0.0; 4],
            ui_muted: [0.0; 4],
        };
        scheme.ansi[1] = [0.8, 0.0, 0.0, 1.0]; // red
        scheme
    }

    #[test]
    fn plain_cell_resolves_fg_bg_unswapped() {
        let scheme = test_scheme();
        let (fg, bg) = resolve_cell_colors(
            AnsiColor::Named(NamedColor::Foreground),
            AnsiColor::Named(NamedColor::Background),
            Flags::empty(),
            false,
            &scheme,
        );
        assert_eq!(fg, scheme.foreground);
        assert_eq!(bg, scheme.background);
    }

    #[test]
    fn inverse_flag_swaps_fg_bg() {
        let scheme = test_scheme();
        let (fg, bg) = resolve_cell_colors(
            AnsiColor::Named(NamedColor::Foreground),
            AnsiColor::Named(NamedColor::Background),
            Flags::INVERSE,
            false,
            &scheme,
        );
        assert_eq!(fg, scheme.background);
        assert_eq!(bg, scheme.foreground);
    }

    #[test]
    fn selection_swaps_fg_bg_after_inverse() {
        let scheme = test_scheme();
        let (fg, bg) = resolve_cell_colors(
            AnsiColor::Named(NamedColor::Red),
            AnsiColor::Named(NamedColor::Background),
            Flags::empty(),
            true,
            &scheme,
        );
        // Selection swap applies to the post-inverse (fg, bg): here inverse
        // didn't fire, so plain fg=red/bg=background gets swapped to
        // fg=background/bg=red.
        assert_eq!(fg, scheme.background);
        assert_eq!(bg, scheme.ansi[1]);
    }

    #[test]
    fn inverse_and_selection_compose() {
        let scheme = test_scheme();
        // Inverse swaps fg/bg first (fg=background, bg=red), then selection
        // swaps again (fg=red, bg=background) -- two swaps cancel out.
        let (fg, bg) = resolve_cell_colors(
            AnsiColor::Named(NamedColor::Red),
            AnsiColor::Named(NamedColor::Background),
            Flags::INVERSE,
            true,
            &scheme,
        );
        assert_eq!(fg, scheme.ansi[1]);
        assert_eq!(bg, scheme.background);
    }
}
```

- [ ] **Step 2: Run the tests to verify they pass**

```bash
cargo test rasterize:: 2>&1 | tail -20
```
Expected: 4 tests pass (this is TDD in the sense of "write the pure logic and its tests together, verify immediately" — `resolve_cell_colors` is simple enough that RED-then-GREEN as two separate steps adds no value here; the test module above already contains the assertions a RED run would motivate).

- [ ] **Step 3: Add the frame cache (moved from `terminal_element.rs`, unchanged) and the rasterization entry point**

Append to `src/gpui_shell/rasterize.rs`, before the `#[cfg(test)]` block:

```rust
thread_local! {
    static SWASH_CACHE: RefCell<SwashCache> = RefCell::new(SwashCache::new());

    // Caches the last rasterized frame per terminal (keyed by `Rc<Terminal>`'s
    // heap address — stable for the terminal's lifetime, and distinct per
    // split pane). Without this, `rasterize_grid` (driven by the ~30Hz
    // repaint poll in `gpui_shell/mod.rs`) would mint a brand-new
    // `RenderImage` — and therefore a brand-new globally-unique `ImageId`
    // (gpui 0.2.2's `assets.rs` static counter) — on every single paint, and
    // gpui's Metal sprite atlas is insert-only: nothing prunes an entry
    // except an explicit `window.drop_image(...)` call. That leaked one
    // full-grid GPU texture per paint, unbounded (measured: 598MB -> 2.52GB
    // RSS in 17s, idle, one terminal — the M0 leak). Caching the previous
    // frame and explicitly dropping it before inserting the next bounds the
    // atlas to one live texture per terminal, and skipping the
    // rasterize+upload entirely when the grid content hasn't changed also
    // avoids needless GPU uploads while idle.
    static LAST_IMAGE: RefCell<HashMap<usize, CachedFrame>> = RefCell::new(HashMap::new());
}

struct CachedFrame {
    /// Hash of the shaped row text, per-cell colors/style, and the bitmap's
    /// pixel dimensions — covers content changes (typing, scrolling, color
    /// changes, selection changes) and resize/rescale (cell size or window
    /// scale factor changing bitmap resolution).
    content_hash: u64,
    image: Arc<RenderImage>,
}

const CURSOR_BLINK_UNUSED: () = (); // placeholder marker removed by Task 3, ignore

/// Drop every cached frame's GPU sprite-atlas entry across all windows —
/// called by `font_state::reload_font_config` when the font changes, since
/// every cached frame is stale the moment the font changes (keyed on content
/// hash, not font identity) and `cache.clear()` alone would free only the
/// Rust-side `Arc<RenderImage>` handles while leaking the underlying GPU
/// texture (gpui's atlas is insert-only — see `LAST_IMAGE`'s doc comment).
pub fn evict_all(cx: &mut App) {
    let evicted: Vec<Arc<RenderImage>> =
        LAST_IMAGE.with_borrow_mut(|cache| cache.drain().map(|(_, frame)| frame.image).collect());
    for image in evicted {
        cx.drop_image(image, None);
    }
}

/// Rasterize `terminal`'s current grid into an RGBA bitmap and upload it as
/// a gpui `RenderImage`, or return the cached one from the last paint if
/// nothing that affects the bitmap has changed. `colors` is the resolved
/// theme (`config.colors`).
pub fn rasterize_grid(
    terminal: &Rc<Terminal>,
    cell_width: Pixels,
    cell_height: Pixels,
    scale: f32,
    colors: &ColorScheme,
    window: &mut Window,
) -> Option<Arc<RenderImage>> {
    terminal.with_term(|term| {
        let content = term.renderable_content();
        let cols = terminal.cols as usize;
        let rows = terminal.rows as usize;
        if cols == 0 || rows == 0 {
            return None;
        }

        let sel_range: Option<SelectionRange> =
            term.selection.as_ref().and_then(|s| s.to_range(term));

        // Ligatures (e.g. `->`, `==`) only exist as a shaper decision across
        // ADJACENT characters in one shaped run, so text is still built one
        // string per row. Colors/style are collected in parallel, one entry
        // per character in that row's string (indices stay aligned with
        // `grid_rows[row].chars()`).
        let mut grid_rows: Vec<String> = vec![String::new(); rows];
        let mut grid_colors: Vec<Vec<([f32; 4], [f32; 4], CellStyle)>> =
            (0..rows).map(|_| Vec::with_capacity(cols)).collect();
        for cell in content.display_iter {
            let row = cell.point.line.0;
            if row < 0 {
                continue;
            }
            let row = row as usize;
            let col = cell.point.column.0;
            if row >= rows || col >= cols {
                continue;
            }
            let row_text = &mut grid_rows[row];
            let row_colors = &mut grid_colors[row];
            while row_text.chars().count() < col {
                row_text.push(' ');
                row_colors.push((colors.foreground, colors.background, CellStyle::NORMAL));
            }
            row_text.push(cell.c);
            let in_selection = sel_range.is_some_and(|range| {
                if range.is_block {
                    cell.point.line >= range.start.line
                        && cell.point.line <= range.end.line
                        && cell.point.column >= range.start.column
                        && cell.point.column <= range.end.column
                } else {
                    cell.point >= range.start && cell.point <= range.end
                }
            });
            let (fg, bg) = resolve_cell_colors(cell.fg, cell.bg, cell.flags, in_selection, colors);
            let style = CellStyle {
                bold: cell.flags.contains(Flags::BOLD),
                italic: cell.flags.contains(Flags::ITALIC),
            };
            row_colors.push((fg, bg, style));
        }

        let cell_w_px = f32::from(cell_width) * scale;
        let cell_h_px = f32::from(cell_height) * scale;
        let bitmap_width = ((cell_w_px * cols as f32).round() as u32).max(1);
        let bitmap_height = ((cell_h_px * rows as f32).round() as u32).max(1);
        let font_size_px = crate::gpui_shell::font_state::font_size() * scale;

        let content_hash = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            grid_rows.hash(&mut hasher);
            for row in &grid_colors {
                for (fg, bg, style) in row {
                    fg.map(|c| c.to_bits()).hash(&mut hasher);
                    bg.map(|c| c.to_bits()).hash(&mut hasher);
                    style.bold.hash(&mut hasher);
                    style.italic.hash(&mut hasher);
                }
            }
            bitmap_width.hash(&mut hasher);
            bitmap_height.hash(&mut hasher);
            hasher.finish()
        };
        let terminal_key = Rc::as_ptr(terminal) as usize;

        let cached = LAST_IMAGE.with_borrow(|cache| {
            cache.get(&terminal_key).and_then(|frame| {
                (frame.content_hash == content_hash).then(|| frame.image.clone())
            })
        });
        if let Some(image) = cached {
            // Grid unchanged since the last paint — reuse the cached
            // texture, skipping cosmic-text shaping/rasterization and the
            // GPU upload entirely.
            return Some(image);
        }

        let mut rgba_image = RgbaImage::new(bitmap_width, bitmap_height);

        // Per-cell background: filled before glyphs are drawn, so text paints
        // on top of its own cell's resolved background.
        for (row_idx, row_colors) in grid_colors.iter().enumerate() {
            for (col_idx, (_, bg, _)) in row_colors.iter().enumerate() {
                if *bg == colors.background {
                    continue; // default background already covers this pixel via the base fill in terminal_element.rs
                }
                let px0 = (col_idx as f32 * cell_w_px).round() as u32;
                let py0 = (row_idx as f32 * cell_h_px).round() as u32;
                let px1 = ((col_idx as f32 + 1.0) * cell_w_px).round().min(bitmap_width as f32) as u32;
                let py1 = ((row_idx as f32 + 1.0) * cell_h_px).round().min(bitmap_height as f32) as u32;
                let pixel = image::Rgba(to_u8_rgba(*bg));
                for y in py0..py1.min(bitmap_height) {
                    for x in px0..px1.min(bitmap_width) {
                        rgba_image.put_pixel(x, y, pixel);
                    }
                }
            }
        }

        crate::gpui_shell::font_state::with_font_system(|font_system, actual_family| {
            SWASH_CACHE.with_borrow_mut(|swash_cache| {
                let metrics = cosmic_text::Metrics::new(font_size_px, cell_h_px);
                let mut buffer = Buffer::new(font_system, metrics);
                let mut buffer = buffer.borrow_with(font_system);
                buffer.set_size(Some(bitmap_width as f32), Some(bitmap_height as f32));
                buffer.set_wrap(Wrap::None);

                // MonoLisa gates its `->`/`==`/`!=`/`>=`-style ligatures
                // behind OpenType Character Variants, NOT calt/liga/ss0x
                // (confirmed via MonoLisa's own specimen page: "Arrows
                // (cv08)", "Equal combinations (cv09)").
                let mut font_features = FontFeatures::new();
                for tag in [
                    b"cv01", b"cv02", b"cv03", b"cv04", b"cv05", b"cv06", b"cv07", b"cv08", b"cv09",
                ] {
                    font_features.enable(FeatureTag::new(tag));
                }

                // Per-row: split into per-(color, style) spans and shape each
                // row as one multi-attrs rich-text pass, so ligatures still
                // shape correctly within a span (a color boundary landing
                // mid-ligature is the same accepted edge case the wgpu
                // renderer already lives with).
                let mut spans_owner: Vec<Vec<(String, Attrs)>> = Vec::with_capacity(rows);
                for (row_idx, row_text) in grid_rows.iter().enumerate() {
                    let chars: Vec<char> = row_text.chars().collect();
                    let row_colors = &grid_colors[row_idx];
                    let mut spans: Vec<(String, Attrs)> = Vec::new();
                    let mut span_text = String::new();
                    let mut span_key: Option<([f32; 4], bool, bool)> = None;
                    for (i, ch) in chars.iter().enumerate() {
                        let (fg, _bg, style) = row_colors
                            .get(i)
                            .copied()
                            .unwrap_or((colors.foreground, colors.background, CellStyle::NORMAL));
                        let key = (fg, style.bold, style.italic);
                        if span_key.is_some() && span_key != Some(key) {
                            spans.push((
                                std::mem::take(&mut span_text),
                                attrs_for(span_key.unwrap(), actual_family, &font_features),
                            ));
                        }
                        span_key = Some(key);
                        span_text.push(*ch);
                    }
                    if let Some(key) = span_key {
                        spans.push((span_text, attrs_for(key, actual_family, &font_features)));
                    }
                    if spans.is_empty() {
                        spans.push((
                            String::new(),
                            attrs_for(
                                (colors.foreground, false, false),
                                actual_family,
                                &font_features,
                            ),
                        ));
                    }
                    spans_owner.push(spans);
                }

                let default_attrs = Attrs::new().family(Family::Name(actual_family));
                for (row_idx, spans) in spans_owner.iter().enumerate() {
                    let rich_spans: Vec<(&str, Attrs)> =
                        spans.iter().map(|(t, a)| (t.as_str(), a.clone())).collect();
                    buffer.set_rich_text(rich_spans, &default_attrs, Shaping::Advanced, None);
                    buffer.shape_until_scroll(true);

                    let row_y_offset = row_idx as f32 * cell_h_px;
                    buffer.draw(swash_cache, CosmicColor::rgb(255, 255, 255), |x, y, w, h, color| {
                        if color.a() == 0 {
                            return;
                        }
                        let py_base = y + row_y_offset.round() as i32;
                        let pixel = image::Rgba([color.r(), color.g(), color.b(), color.a()]);
                        for off_y in 0..h {
                            let py = py_base + off_y as i32;
                            if py < 0 || py as u32 >= bitmap_height {
                                continue;
                            }
                            for off_x in 0..w {
                                let px_x = x + off_x as i32;
                                if px_x < 0 || px_x as u32 >= bitmap_width {
                                    continue;
                                }
                                rgba_image.put_pixel(px_x as u32, py as u32, pixel);
                            }
                        }
                    });
                    // Reset the buffer for the next row's single-line shape;
                    // `set_rich_text` above replaces the buffer's lines each
                    // call, so this loop shapes and draws one row at a time
                    // rather than the whole grid as one multi-line buffer —
                    // simpler than tracking per-line Y offsets inside cosmic-text's
                    // own layout, and each row was already an independent
                    // shaping unit before this task (ligatures don't cross
                    // row boundaries in a terminal grid).
                }
            });
        });

        // gpui's sprite atlas stores images as BGRA with straight alpha
        // (verified against gpui 0.2.2's `elements/img.rs` decode path,
        // which does the same swap for plain RGBA-decoded images).
        for pixel in rgba_image.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }

        let image = Arc::new(RenderImage::new(smallvec::smallvec![Frame::new(rgba_image)]));

        let old = LAST_IMAGE.with_borrow_mut(|cache| {
            cache.insert(
                terminal_key,
                CachedFrame {
                    content_hash,
                    image: image.clone(),
                },
            )
        });
        // Explicitly free the previous frame's sprite-atlas entry — gpui's
        // atlas is insert-only otherwise, which is the root cause of the M0
        // leak this cache fixes.
        if let Some(old_frame) = old {
            let _ = window.drop_image(old_frame.image);
        }

        Some(image)
    })
}

fn attrs_for<'a>(
    (fg, bold, italic): ([f32; 4], bool, bool),
    family: &'a str,
    font_features: &FontFeatures,
) -> Attrs<'a> {
    let [r, g, b, a] = to_u8_rgba(fg);
    let mut attrs = Attrs::new()
        .family(Family::Name(family))
        .color(CosmicColor::rgba(r, g, b, a))
        .font_features(font_features.clone());
    if bold {
        attrs = attrs.weight(Weight::BOLD);
    }
    if italic {
        attrs = attrs.style(Style::Italic);
    }
    attrs
}

fn to_u8_rgba(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[3].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}
```

Both APIs this step relies on are verified against real cosmic-text 0.18.2 source (checked during plan-writing, not assumed): `BorrowedWithFontSystem<Buffer>::set_rich_text(spans: impl IntoIterator<Item = (&str, Attrs)>, default_attrs: &Attrs, shaping: Shaping, alignment: Option<Align>)` — the call above passing `None` for `alignment` matches exactly. `Color::rgba(r: u8, g: u8, b: u8, a: u8) -> Self` is the real public constructor (packs `(a<<24)|(r<<16)|(g<<8)|b`) — used directly above via `to_u8_rgba` rather than hand-packing the bits.

- [ ] **Step 4: Update `terminal_element.rs`: delete the moved code, call `rasterize::rasterize_grid`**

Delete from `terminal_element.rs`: the `SWASH_CACHE`/`LAST_IMAGE` `thread_local!` block, the `CachedFrame` struct, and the `evict_all_frames` function (added in Task 1 — Step 3 above is where it landed in `rasterize.rs` as `evict_all`, so this is a straight relocation, not new logic).

Replace the entire block from `self.terminal.with_term(|term| { ... });` (the row-building + rasterization + cache logic) with:

```rust
if let Some(render_image) = rasterize::rasterize_grid(
    &self.terminal,
    self.cell_width,
    self.cell_height,
    window.scale_factor(),
    &self.colors,
    window,
) {
    let _ = window.paint_image(bounds, Corners::default(), render_image, 0, false);
}
```

This means `TerminalGridElement` needs a new field `colors: ColorScheme` (the resolved theme) — add it to the struct:

```rust
pub struct TerminalGridElement {
    pub terminal: Rc<Terminal>,
    pub cell_width: Pixels,
    pub cell_height: Pixels,
    pub colors: crate::config::schema::ColorScheme,
}
```

Add `mod rasterize;` to `src/gpui_shell/mod.rs` (private — only `terminal_element.rs` and `font_state.rs` call into it, both within the crate), and update `mod.rs`'s `render()` to pass `colors: self.config.colors.clone()` when constructing each `TerminalGridElement`.

Finally, update `font_state.rs`'s `reload_font_config` (Task 1) to call the relocated function:

```rust
crate::gpui_shell::rasterize::evict_all(cx);
```

replacing `crate::gpui_shell::terminal_element::evict_all_frames(cx);` — and update the line just above it in that function's doc comment (`"see terminal_element::evict_all_frames's doc comment"`) to point at `rasterize::evict_all` instead.

- [ ] **Step 5: Build, fix errors, verify the full gate**

```bash
cargo build --bin gpui-petruterm 2>&1
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```

- [ ] **Step 6: Dogfood checkpoint — STOP and ask the user to confirm**

Ask the user to run `cargo run --bin gpui-petruterm` and:
1. Run `ls --color=auto` (or any colorized command) and confirm colors render (not all-white/monochrome text).
2. Run something with bold/italic (e.g. `printf '\033[1mbold\033[0m \033[3mitalic\033[0m\n'`) and confirm the visual distinction (weight/slant) shows up — not just color changes.
3. Confirm ligatures still work (`-> == != >=` still combine into single glyphs) — this task rewrote the shaping loop, so this is a real regression risk to check, not a formality.
4. Report back which of the two flagged uncertainties in Step 3 (if any) needed a fix.

- [ ] **Step 7: Commit**

```bash
git add src/gpui_shell/rasterize.rs src/gpui_shell/terminal_element.rs src/gpui_shell/mod.rs
git commit -m "[gpui-migration] feat: ANSI colors + selection-highlight rendering (M1b).

The gpui grid previously rendered every cell in one fixed foreground/
background -- cell.fg/cell.bg were never read. Extracts rasterization
out of terminal_element.rs into a new rasterize.rs (the file was
already over the project's 400-line convention; this is the single
biggest addition this plan makes to it) and adds real per-cell color
resolution, ported from src/app/mux/mod.rs's row-building loop:
inverse-video swap (Flags::INVERSE), resolved via the existing,
PTY-agnostic term::color::resolve_color, plus bold/italic via
font::shaper::CellStyle.

Selection-highlight rendering (fg/bg swap for cells inside
term.selection) is implemented here too, even though nothing creates
a selection until a later task in this plan -- Terminal::start_selection/
update_selection already exist as pre-M1b infrastructure, and
term.selection is free to read from the same with_term closure this
task is already inside. The selection-input task needs zero rendering
changes as a result.

Per-color/per-style spans are shaped per row (not one Attrs for the
whole row) so ligatures still shape correctly within a span -- a
color boundary landing mid-ligature is the same accepted edge case
the wgpu renderer already lives with.

scripts/ci-local.sh clean. Dogfooded: colors render (ls --color),
bold/italic render, ligatures still combine correctly."
```

---

### Task 3: Cursor shapes + blink

**Files:**
- Modify: `src/gpui_shell/terminal_element.rs` (cursor-painting block at the end of `paint()`)
- Modify: `src/gpui_shell/mod.rs` (poll loop gains a blink toggle; `GpuiShellRoot` gains `cursor_blink_on: bool`/`cursor_last_blink: Instant`; `render()` passes `is_active`/`cursor_blink_on` into each `TerminalGridElement`)

**Interfaces:**
- Consumes: `Terminal::cursor_info() -> CursorInfo { col, row, shape: CursorShape, visible: bool }` (already exists, unchanged).
- Produces: `TerminalGridElement` gains fields `is_active: bool`, `cursor_blink_on: bool`. `GpuiShellRoot` gains fields `cursor_blink_on: bool`, `cursor_last_blink: std::time::Instant`. Later tasks (4, 6) that construct `TerminalGridElement` in `mod.rs`'s `render()` must include these two new fields alongside `colors` (Task 2) — the compiler will enforce this (missing-field errors) if a later task's diff doesn't account for it.

- [ ] **Step 1: Replace the cursor-painting block in `terminal_element.rs`'s `paint()`**

Replace:

```rust
        // Cursor.
        let cursor = self.terminal.cursor_info();
        if cursor.visible {
            let cursor_origin = point(
                bounds.origin.x + self.cell_width * (cursor.col as f32),
                bounds.origin.y + self.cell_height * (cursor.row as f32),
            );
            window.paint_quad(fill(
                Bounds {
                    origin: cursor_origin,
                    size: size(self.cell_width, self.cell_height),
                },
                gpui::rgba(0xf8f8f280),
            ));
        }
```

with:

```rust
        // Cursor. Shape from Terminal::cursor_info() (DECSCUSR / default),
        // geometry ported from src/app/renderer/terminal.rs's
        // build_cursor_overlay. HollowBlock swaps in for Block when this
        // pane isn't the split-focus target -- matches the wgpu renderer's
        // convention for showing which pane has keyboard focus.
        let cursor = self.terminal.cursor_info();
        if cursor.visible && self.cursor_blink_on {
            use alacritty_terminal::vte::ansi::CursorShape;
            let shape = if !self.is_active && cursor.shape == CursorShape::Block {
                CursorShape::HollowBlock
            } else {
                cursor.shape
            };
            let cell_w = self.cell_width;
            let cell_h = self.cell_height;
            let cursor_origin = point(
                bounds.origin.x + cell_w * (cursor.col as f32),
                bounds.origin.y + cell_h * (cursor.row as f32),
            );
            let (offset, geom_size) = match shape {
                CursorShape::Block | CursorShape::HollowBlock => {
                    (point(px(0.0), px(0.0)), size(cell_w, cell_h))
                }
                CursorShape::Underline => (
                    point(px(0.0), (cell_h - px(2.0)).max(px(0.0))),
                    size(cell_w, px(2.0)),
                ),
                CursorShape::Beam => (point(px(0.0), px(0.0)), size(px(2.0), cell_h)),
                CursorShape::Hidden => return,
            };
            let quad_bounds = Bounds {
                origin: point(cursor_origin.x + offset.x, cursor_origin.y + offset.y),
                size: geom_size,
            };
            if shape == CursorShape::HollowBlock {
                // Outline only -- four thin edge rects, not a filled quad,
                // so the cell's own content stays visible underneath.
                let t = px(1.0);
                let color = gpui::rgba(0xf8f8f2ff);
                window.paint_quad(fill(
                    Bounds { origin: quad_bounds.origin, size: size(geom_size.width, t) },
                    color,
                ));
                window.paint_quad(fill(
                    Bounds {
                        origin: point(quad_bounds.origin.x, quad_bounds.origin.y + geom_size.height - t),
                        size: size(geom_size.width, t),
                    },
                    color,
                ));
                window.paint_quad(fill(
                    Bounds { origin: quad_bounds.origin, size: size(t, geom_size.height) },
                    color,
                ));
                window.paint_quad(fill(
                    Bounds {
                        origin: point(quad_bounds.origin.x + geom_size.width - t, quad_bounds.origin.y),
                        size: size(t, geom_size.height),
                    },
                    color,
                ));
            } else {
                window.paint_quad(fill(quad_bounds, gpui::rgba(0xf8f8f280)));
            }
        }
```

Add the two new fields to `TerminalGridElement`'s struct definition (alongside `colors` from Task 2):

```rust
pub struct TerminalGridElement {
    pub terminal: Rc<Terminal>,
    pub cell_width: Pixels,
    pub cell_height: Pixels,
    pub colors: crate::config::schema::ColorScheme,
    pub is_active: bool,
    pub cursor_blink_on: bool,
}
```

- [ ] **Step 2: Add blink state to `GpuiShellRoot` and the poll loop, in `mod.rs`**

Add fields to the struct:

```rust
pub struct GpuiShellRoot {
    pub terminals: Vec<Rc<Terminal>>,
    pub focus_handle: FocusHandle,
    active_terminal: usize,
    config: Config,
    wakeup_gates: Vec<Arc<WakeupGate>>,
    cursor_blink_on: bool,
    cursor_last_blink: std::time::Instant,
}
```

Initialize in `GpuiShellRoot::new`'s `Self { ... }` construction:

```rust
        Self {
            terminals: vec![terminal],
            focus_handle: cx.focus_handle(),
            active_terminal: 0,
            config,
            wakeup_gates: vec![gate],
            cursor_blink_on: true,
            cursor_last_blink: std::time::Instant::now(),
        }
```

Add the blink toggle to the poll loop, in the `let alive = this.update(...)` closure (the last one in the loop body):

```rust
                let alive = this
                    .update(cx, |this: &mut Self, cx| {
                        let mut should_notify = this.wakeup_gates.iter().any(|g| g.take_pending());
                        // Blink at the same 530ms cadence the wgpu app uses
                        // (Input::update_cursor_blink). Piggybacks on this
                        // already-running 33ms poll loop instead of a new
                        // timer.
                        if this.cursor_last_blink.elapsed() >= std::time::Duration::from_millis(530) {
                            this.cursor_blink_on = !this.cursor_blink_on;
                            this.cursor_last_blink = std::time::Instant::now();
                            should_notify = true;
                        }
                        if should_notify {
                            cx.notify();
                        }
                    })
                    .is_ok();
```

Reset to visible-on on any keystroke, in `on_key_down` (add at the top of the function body, before the existing logic):

```rust
    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor_blink_on = true;
        self.cursor_last_blink = std::time::Instant::now();
        let Some(terminal) = self.terminals.get(self.active_terminal) else {
```

- [ ] **Step 3: Update `render()`'s `.children(...)` closure to pass the new fields**

```rust
            .children(self.terminals.iter().enumerate().map(|(idx, t)| {
                let (cell_width, cell_height) = font_state::measured_cell_size();
                TerminalGridElement {
                    terminal: t.clone(),
                    cell_width,
                    cell_height,
                    colors: self.config.colors.clone(),
                    is_active: idx == self.active_terminal,
                    cursor_blink_on: self.cursor_blink_on,
                }
            }))
```

- [ ] **Step 4: Build, fix errors, verify the full gate**

```bash
cargo build --bin gpui-petruterm 2>&1
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```

- [ ] **Step 5: Dogfood checkpoint — STOP and ask the user to confirm**

Ask the user to run `cargo run --bin gpui-petruterm` and:
1. Confirm the cursor blinks (visible → invisible → visible, roughly every half second) while idle, and stops blinking (stays visible) while actively typing.
2. Run `vim` (or `nvim`), switch to insert mode and back to normal mode, and confirm the cursor shape actually changes (typically block in normal mode, beam or underline in insert mode, depending on the app's own DECSCUSR sequences) — this is the real regression check for the geometry table.
3. Trigger `ctrl-f %` to open a second split pane, and confirm the un-focused pane's cursor renders as a hollow outline rather than a solid block.

- [ ] **Step 6: Commit**

```bash
git add src/gpui_shell/terminal_element.rs src/gpui_shell/mod.rs
git commit -m "[gpui-migration] feat: Cursor shapes + blink (M1b).

Cursor geometry ported from src/app/renderer/terminal.rs's
build_cursor_overlay: Block/HollowBlock (full cell), Underline
(bottom 2px), Beam (left 2px), Hidden (skip). HollowBlock (outline
only, four thin edge rects) swaps in for Block when a pane isn't the
split-focus target, using the same is_active signal click-to-focus
will need in a later task.

Blink piggybacks on the existing 33ms poll loop in GpuiShellRoot::new
-- a toggle every 530ms (matching the wgpu app's own
update_cursor_blink threshold) calls cx.notify(), reset to visible-on
by any keystroke. No new timer infrastructure.

scripts/ci-local.sh clean. Dogfooded: blink works and pauses while
typing, cursor shape changes with vim's insert/normal mode, unfocused
split pane shows a hollow cursor."
```

---

### Task 4: `mouse.rs` — click-drag selection, copy, click-to-focus

Establishes the mouse-handling architecture (each `TerminalGridElement` registers its own `window.on_mouse_event` handlers, scoped to its own `paint()` bounds) and delivers the first concrete use of it: text selection. Selection *rendering* already works (Task 2) — this task only wires the *input* side.

**Files:**
- Create: `src/gpui_shell/mouse.rs`
- Modify: `src/gpui_shell/terminal_element.rs` (`paint()` registers mouse handlers; struct gains `on_focus` callback field)
- Modify: `src/gpui_shell/mod.rs` (`render()`'s children closure builds the `on_focus` callback via `WeakEntity`)
- Test: `src/gpui_shell/mouse.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `Terminal::start_selection(col: usize, row: usize, ty: SelectionType)`, `update_selection(col: usize, row: usize)`, `selection_text() -> Option<String>`, `clear_selection()` (all already exist, unchanged), `alacritty_terminal::selection::SelectionType` (`Simple`/`Semantic`/`Lines`/`Block` — this task uses `Simple`/`Semantic`/`Lines`).
- Produces: `mouse::register_mouse_handlers(terminal: Rc<Terminal>, bounds: Bounds<Pixels>, cell_width: Pixels, cell_height: Pixels, on_focus: Rc<dyn Fn(&mut Window, &mut App)>, window: &mut Window)` — called from `terminal_element.rs`'s `paint()`. `mouse::register_click(terminal_key: usize, cell: (usize, usize)) -> u32` and `mouse::pixel_to_cell(position: Point<Pixels>, bounds: Bounds<Pixels>, cell_width: Pixels, cell_height: Pixels) -> (usize, usize)` (both pure, unit-tested). `TerminalGridElement` gains field `on_focus: Rc<dyn Fn(&mut Window, &mut App)>`.
- Note for Task 5 (mouse-report passthrough) and Task 6 (scrollback + scrollbar drag): both extend `mouse.rs`'s mouse-down handler registered here — Task 5 adds a check before the selection logic runs, Task 6 adds a check for "did this mouse-down land in the scrollbar strip" before the text-selection logic runs. Both must be read against this task's actual landed code, not guessed from this brief alone.

- [ ] **Step 1: Write the failing tests for the pure logic**

Create `src/gpui_shell/mouse.rs`:

```rust
// gpui chrome migration (M1b): mouse-driven interaction for the terminal
// grid -- click-drag selection, click-to-focus for split panes, and (later
// tasks in this plan) mouse-report passthrough and scrollbar drag. Each
// `TerminalGridElement` owns its own mouse handling, registered fresh every
// `paint()` call and scoped to that element's own `bounds` -- mirrors how
// the element already owns cursor/text painting math scoped to its own
// bounds, and avoids tracking child-element bounds in the parent `div` just
// for hit-testing (gpui's flex layout doesn't expose child bounds until
// paint completes).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use alacritty_terminal::selection::SelectionType;
use gpui::{App, Bounds, DispatchPhase, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Window};

use crate::term::Terminal;

/// Per-terminal click-tracking state, keyed the same way `rasterize`'s
/// `LAST_IMAGE` is (the `Rc<Terminal>`'s heap address) -- `TerminalGridElement`
/// is rebuilt every frame, so this can't live on the element itself.
struct ClickState {
    last_click_time: Instant,
    last_click_cell: (usize, usize),
    click_count: u32,
}

thread_local! {
    static CLICK_STATE: RefCell<HashMap<usize, ClickState>> = RefCell::new(HashMap::new());
}

/// Update click count for multi-click detection at `cell`, keyed by
/// `terminal_key` (an `Rc<Terminal>` heap address, matching `rasterize`'s
/// `LAST_IMAGE` key). Returns 1 / 2 / 3 based on timing and position --
/// ported from `src/app/input/mod.rs`'s `register_click` as-is.
pub fn register_click(terminal_key: usize, cell: (usize, usize)) -> u32 {
    const DOUBLE_CLICK_MS: u128 = 500;
    CLICK_STATE.with_borrow_mut(|states| {
        let state = states.entry(terminal_key).or_insert(ClickState {
            last_click_time: Instant::now() - std::time::Duration::from_secs(1),
            last_click_cell: (usize::MAX, usize::MAX),
            click_count: 0,
        });
        let same_cell = state.last_click_cell == cell;
        let within_time = state.last_click_time.elapsed().as_millis() < DOUBLE_CLICK_MS;
        state.click_count = if same_cell && within_time {
            (state.click_count + 1).min(3)
        } else {
            1
        };
        state.last_click_time = Instant::now();
        state.last_click_cell = cell;
        state.click_count
    })
}

/// Map a click count to the alacritty selection type it starts -- ported
/// from `src/app/mod.rs`'s mapping as-is.
pub fn selection_type_for_clicks(clicks: u32) -> SelectionType {
    match clicks {
        2 => SelectionType::Semantic,
        3 => SelectionType::Lines,
        _ => SelectionType::Simple,
    }
}

/// Convert a window-relative mouse position to a (col, row) grid cell,
/// relative to `bounds`'s origin -- simpler than the wgpu app's
/// `pixel_to_cell` (no pane padding to account for; `bounds` is already
/// this element's own painted area).
pub fn pixel_to_cell(
    position: Point<Pixels>,
    bounds: Bounds<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
) -> (usize, usize) {
    let x = f32::from(position.x - bounds.origin.x);
    let y = f32::from(position.y - bounds.origin.y);
    let col = (x / f32::from(cell_width)).floor().max(0.0) as usize;
    let row = (y / f32::from(cell_height)).floor().max(0.0) as usize;
    (col, row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px};

    #[test]
    fn first_click_is_count_one() {
        assert_eq!(register_click(1, (5, 5)), 1);
    }

    #[test]
    fn same_cell_quick_second_click_is_count_two() {
        let key = 2;
        register_click(key, (5, 5));
        assert_eq!(register_click(key, (5, 5)), 2);
    }

    #[test]
    fn different_cell_resets_to_count_one() {
        let key = 3;
        register_click(key, (5, 5));
        assert_eq!(register_click(key, (6, 5)), 1);
    }

    #[test]
    fn click_count_caps_at_three() {
        let key = 4;
        register_click(key, (5, 5));
        register_click(key, (5, 5));
        register_click(key, (5, 5));
        assert_eq!(register_click(key, (5, 5)), 3);
    }

    #[test]
    fn click_count_maps_to_selection_type() {
        assert_eq!(selection_type_for_clicks(1), SelectionType::Simple);
        assert_eq!(selection_type_for_clicks(2), SelectionType::Semantic);
        assert_eq!(selection_type_for_clicks(3), SelectionType::Lines);
    }

    #[test]
    fn pixel_to_cell_is_bounds_relative() {
        let bounds = Bounds {
            origin: point(px(100.0), px(50.0)),
            size: gpui::size(px(800.0), px(600.0)),
        };
        let cell = pixel_to_cell(point(px(109.0), px(66.0)), bounds, px(9.0), px(18.0));
        assert_eq!(cell, (1, 0)); // (109-100)/9 = 1.0, (66-50)/18 = 0.888 -> row 0
    }
}
```

- [ ] **Step 2: Run the tests to verify they pass**

```bash
cargo test gpui_shell::mouse:: 2>&1 | tail -30
```
Expected: 6 tests pass.

- [ ] **Step 3: Add the mouse-event registration (selection drag + click-to-focus)**

Append to `src/gpui_shell/mouse.rs`, before `#[cfg(test)]`:

```rust
/// Register this element's mouse handlers for the current frame (cleared
/// automatically by gpui after paint -- must be called fresh every
/// `paint()`, per `Window::on_mouse_event`'s own contract). Handles
/// click-drag selection and click-to-focus; later tasks in this plan add
/// mouse-report passthrough and scrollbar-drag checks before this task's
/// selection logic runs.
pub fn register_mouse_handlers(
    terminal: Rc<Terminal>,
    bounds: Bounds<Pixels>,
    cell_width: Pixels,
    cell_height: Pixels,
    on_focus: Rc<dyn Fn(&mut Window, &mut App)>,
    window: &mut Window,
) {
    let terminal_key = Rc::as_ptr(&terminal) as usize;

    let down_terminal = terminal.clone();
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
            return;
        }
        if !bounds.contains(&event.position) {
            return;
        }
        on_focus(window, cx);
        let (col, row) = pixel_to_cell(event.position, bounds, cell_width, cell_height);
        let clicks = register_click(terminal_key, (col, row));
        down_terminal.start_selection(col, row, selection_type_for_clicks(clicks));
    });

    let move_terminal = terminal.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, _cx| {
        if phase != DispatchPhase::Bubble {
            return;
        }
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        if !bounds.contains(&event.position) {
            return;
        }
        let (col, row) = pixel_to_cell(event.position, bounds, cell_width, cell_height);
        move_terminal.update_selection(col, row);
    });

    let up_terminal = terminal;
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
            return;
        }
        if let Some(text) = up_terminal.selection_text() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        }
    });
}
```

`Bounds<Pixels>::contains(&self, point: &Point<Pixels>) -> bool` is verified against real gpui 0.2.2 source (`src/geometry.rs`) — matches the calls above exactly.

- [ ] **Step 4: Wire it into `terminal_element.rs`'s `paint()`**

Add near the top of `paint()`, after the background fill:

```rust
        mouse::register_mouse_handlers(
            self.terminal.clone(),
            bounds,
            self.cell_width,
            self.cell_height,
            self.on_focus.clone(),
            window,
        );
```

Add `on_focus` to the struct:

```rust
pub struct TerminalGridElement {
    pub terminal: Rc<Terminal>,
    pub cell_width: Pixels,
    pub cell_height: Pixels,
    pub colors: crate::config::schema::ColorScheme,
    pub is_active: bool,
    pub cursor_blink_on: bool,
    pub on_focus: std::rc::Rc<dyn Fn(&mut gpui::Window, &mut gpui::App)>,
}
```

Add `mod mouse;` to `src/gpui_shell/mod.rs` (private, alongside `mod rasterize;`).

- [ ] **Step 5: Build the `on_focus` callback in `mod.rs`'s `render()`**

```rust
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.focus(&self.focus_handle);
        let weak = cx.weak_entity();
        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_action(cx.listener(Self::on_split_demo))
            .flex()
            .size_full()
            .children(self.terminals.iter().enumerate().map(|(idx, t)| {
                let (cell_width, cell_height) = font_state::measured_cell_size();
                let weak = weak.clone();
                let on_focus: Rc<dyn Fn(&mut Window, &mut App)> = Rc::new(move |_window, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        if this.active_terminal != idx {
                            this.active_terminal = idx;
                            cx.notify();
                        }
                    });
                });
                TerminalGridElement {
                    terminal: t.clone(),
                    cell_width,
                    cell_height,
                    colors: self.config.colors.clone(),
                    is_active: idx == self.active_terminal,
                    cursor_blink_on: self.cursor_blink_on,
                    on_focus,
                }
            }))
    }
```

`Context::weak_entity(&self) -> WeakEntity<T>` is verified against real gpui 0.2.2 source (`src/app/context.rs`) — matches the call above exactly.

- [ ] **Step 6: Build, fix errors, verify the full gate**

```bash
cargo build --bin gpui-petruterm 2>&1
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```

- [ ] **Step 7: Dogfood checkpoint — STOP and ask the user to confirm**

Ask the user to run `cargo run --bin gpui-petruterm` and:
1. Click-drag over some text and confirm it highlights (using the selection-swap rendering Task 2 already built).
2. Double-click a word and confirm the whole word gets selected; triple-click a line and confirm the whole line does.
3. After selecting, confirm the text is on the system clipboard (paste it somewhere).
4. Open a second split pane (`ctrl-f %`) and click into the first pane — confirm its cursor becomes the active (solid Block) one and the second pane's goes hollow (this exercises `on_focus`, wired through this task).

- [ ] **Step 8: Commit**

```bash
git add src/gpui_shell/mouse.rs src/gpui_shell/terminal_element.rs src/gpui_shell/mod.rs
git commit -m "[gpui-migration] feat: Click-drag selection + copy + click-to-focus (M1b).

Establishes the mouse-handling architecture this plan's remaining
tasks build on: each TerminalGridElement registers its own
window.on_mouse_event handlers inside paint(), scoped to its own
bounds -- mirrors how the element already owns cursor/text painting
math scoped to its own bounds, avoiding a parent-div bounds-tracking
side-channel gpui's flex layout doesn't expose until paint completes.

Click-count tracking (register_click, 500ms same-cell window, capped
at 3) and the click-count -> SelectionType mapping (1=Simple,
2=Semantic, 3=Lines) are ported from src/app/input/mod.rs and
src/app/mod.rs as-is. Selection *rendering* already worked (a prior
task in this plan) -- this is only the input side: mouse-down starts
a selection, mouse-move while dragging extends it, mouse-up copies
the selected text via gpui's own cx.write_to_clipboard (not the wgpu
app's arboard dependency).

Click-to-focus (on_focus callback, built via WeakEntity in
GpuiShellRoot::render) lets a split pane become the active terminal
on click -- the first real consumer of the is_active signal added
for cursor hollow-vs-block rendering.

scripts/ci-local.sh clean. Dogfooded: click-drag/double/triple-click
selection all work, copy works, click-to-focus switches the active
pane's cursor correctly."
```

---

### Task 5: Mouse-report passthrough

**Files:**
- Modify: `src/gpui_shell/mouse.rs` (mouse-down/move/up handlers gain a `mouse_mode_flags()` check before the Task 4 selection logic)
- Test: `src/gpui_shell/mouse.rs` (extend `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `Terminal::mouse_mode_flags() -> (bool, bool, bool)` (`(any_reporting, sgr, motion)`, already exists, unchanged), `Terminal::write_input(&self, data: &[u8])` (already exists).
- Produces: `mouse::format_mouse_report(button: u8, col: usize, row: usize, pressed: bool, sgr: bool) -> Option<Vec<u8>>` (pure, unit-tested; `None` for the legacy-X10 release case, which sends nothing per the ported behavior).

- [ ] **Step 1: Write the failing tests**

Add to `src/gpui_shell/mouse.rs`'s `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn sgr_press_format() {
        let bytes = format_mouse_report(0, 4, 9, true, true).unwrap();
        assert_eq!(bytes, b"\x1b[<0;5;10M");
    }

    #[test]
    fn sgr_release_format() {
        let bytes = format_mouse_report(0, 4, 9, false, true).unwrap();
        assert_eq!(bytes, b"\x1b[<0;5;10m");
    }

    #[test]
    fn legacy_x10_press_format() {
        let bytes = format_mouse_report(0, 4, 9, true, false).unwrap();
        assert_eq!(bytes, &[0x1b, b'[', b'M', 32, 5 + 32, 10 + 32]);
    }

    #[test]
    fn legacy_x10_release_sends_nothing() {
        assert_eq!(format_mouse_report(0, 4, 9, false, false), None);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test gpui_shell::mouse::tests::sgr_press_format 2>&1 | tail -10
```
Expected: FAIL with "cannot find function `format_mouse_report`".

- [ ] **Step 3: Implement `format_mouse_report`, wire the passthrough check into mouse-down/move/up**

Add above the `#[cfg(test)]` block:

```rust
/// Format a mouse-report escape sequence for `button` at (col, row),
/// `pressed` or released, in SGR or legacy X10 mode -- ported from
/// `src/app/input/mod.rs`'s `send_mouse_report` as-is. Legacy X10 mode
/// only reports presses (returns `None` on release, matching the original).
pub fn format_mouse_report(button: u8, col: usize, row: usize, pressed: bool, sgr: bool) -> Option<Vec<u8>> {
    if sgr {
        let c = if pressed { 'M' } else { 'm' };
        Some(format!("\x1b[<{button};{};{}{c}", col + 1, row + 1).into_bytes())
    } else if pressed {
        let b = button.saturating_add(32);
        let x = ((col + 1) as u8).saturating_add(32);
        let y = ((row + 1) as u8).saturating_add(32);
        Some(vec![0x1b, b'[', b'M', b, x, y])
    } else {
        None
    }
}
```

In `register_mouse_handlers` (from Task 4), add the passthrough check as the *first* thing each handler does — before the click-count/selection logic:

```rust
    let down_terminal = terminal.clone();
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
            return;
        }
        if !bounds.contains(&event.position) {
            return;
        }
        let (col, row) = pixel_to_cell(event.position, bounds, cell_width, cell_height);
        let (any_mouse, sgr, _) = down_terminal.mouse_mode_flags();
        if any_mouse {
            if let Some(bytes) = format_mouse_report(0, col, row, true, sgr) {
                down_terminal.write_input(&bytes);
            }
            return; // mouse-report mode: don't also start a local selection
        }
        on_focus(window, cx);
        let clicks = register_click(terminal_key, (col, row));
        down_terminal.start_selection(col, row, selection_type_for_clicks(clicks));
    });
```

Apply the equivalent guard (check `mouse_mode_flags()` first, `return` before the existing logic if `any_mouse`) to the move and up handlers, sending `format_mouse_report` output on move too if `motion` (the third tuple element) is true — read `register_mouse_handlers`'s actual current code from Task 4's landed commit before making this edit, since this brief only shows the mouse-down handler's exact shape; port the same pattern to move/up consistently with whatever that commit's real code looks like.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test gpui_shell::mouse:: 2>&1 | tail -30
```
Expected: 10 tests pass (6 from Task 4 + 4 new).

- [ ] **Step 5: Build, verify the full gate**

```bash
cargo build --bin gpui-petruterm 2>&1
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```

- [ ] **Step 6: Dogfood checkpoint — STOP and ask the user to confirm**

Ask the user to run `cargo run --bin gpui-petruterm`, then:
1. Run `vim` (or `tmux`) and enable mouse mode (`:set mouse=a` in vim, or tmux's default). Click somewhere in the buffer and confirm vim/tmux registers the click (e.g. vim moves its cursor to the clicked position) rather than the gpui shell drawing a local text selection.
2. Confirm plain click-drag selection (outside vim/tmux, or in a program that doesn't enable mouse mode) still works exactly as it did after Task 4 — this task must not have broken the non-mouse-report path.

- [ ] **Step 7: Commit**

```bash
git add src/gpui_shell/mouse.rs
git commit -m "[gpui-migration] feat: Mouse-report passthrough for vim/tmux (M1b).

Before treating a click as local selection, checks
Terminal::mouse_mode_flags() (already exists) -- if the terminal has
mouse reporting enabled (vim :set mouse=a, tmux's default), sends the
SGR (\\x1b[<{btn};{col};{row}M/m) or legacy X10
(\\x1b[M{btn+32}{col+32}{row+32}, press-only) escape sequence instead
of starting a local selection. Ported from src/app/input/mod.rs's
send_mouse_report as-is.

scripts/ci-local.sh clean, 10/10 mouse.rs tests pass. Dogfooded:
vim mouse mode registers clicks correctly, plain click-drag selection
(outside mouse-mode apps) still works unchanged."
```

---

### Task 6: Scrollback + visual scrollbar

**Files:**
- Modify: `src/gpui_shell/terminal_element.rs` (`paint()` gains scrollbar-quad painting)
- Modify: `src/gpui_shell/mouse.rs` (scroll-wheel handler; scrollbar-thumb-drag check before Task 4's selection logic)
- Test: `src/gpui_shell/mouse.rs` (extend `#[cfg(test)] mod tests` with the thumb-geometry pure function)

**Interfaces:**
- Consumes: `Terminal::scroll_display(delta: i32)`, `scroll_to_bottom()`, `scrollback_info() -> (display_offset: usize, history_size: usize)` (all already exist, unchanged).
- Produces: `mouse::scrollbar_thumb_geometry(screen_rows: usize, history_size: usize, display_offset: usize) -> (usize, usize)` (returns `(thumb_start, thumb_rows)`, pure, unit-tested).

- [ ] **Step 1: Write the failing tests for the thumb geometry**

Add to `src/gpui_shell/mouse.rs`'s `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn no_scrollback_thumb_fills_track() {
        let (start, rows) = scrollbar_thumb_geometry(24, 0, 0);
        assert_eq!((start, rows), (0, 24));
    }

    #[test]
    fn at_bottom_thumb_sits_at_bottom() {
        let (start, rows) = scrollbar_thumb_geometry(24, 100, 0);
        assert!(rows < 24); // thumb shrinks once there's scrollback
        assert_eq!(start + rows, 24); // flush with the bottom of the track
    }

    #[test]
    fn at_top_thumb_sits_at_top() {
        let (start, _rows) = scrollbar_thumb_geometry(24, 100, 100);
        assert_eq!(start, 0);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test gpui_shell::mouse::tests::no_scrollback_thumb_fills_track 2>&1 | tail -10
```
Expected: FAIL with "cannot find function `scrollbar_thumb_geometry`".

- [ ] **Step 3: Implement `scrollbar_thumb_geometry`**

Add above `#[cfg(test)]`:

```rust
/// Scrollbar thumb geometry in row units: `(thumb_start, thumb_rows)`.
/// `display_offset` = 0 means at the bottom of scrollback, `history_size`
/// means at the top -- matches `Terminal::scrollback_info`'s own convention.
/// Ported from `src/app/renderer/overlay.rs`'s `build_scroll_bar_instances`
/// geometry as-is.
pub fn scrollbar_thumb_geometry(
    screen_rows: usize,
    history_size: usize,
    display_offset: usize,
) -> (usize, usize) {
    let total_lines = (screen_rows + history_size).max(1);
    let thumb_rows = (((screen_rows as f32 / total_lines as f32) * screen_rows as f32)
        .round() as usize)
        .clamp(1, screen_rows);
    let slack = screen_rows.saturating_sub(thumb_rows);
    let scroll_frac = if history_size == 0 {
        0.0
    } else {
        display_offset as f32 / history_size as f32
    };
    let thumb_start = ((1.0 - scroll_frac) * slack as f32).round() as usize;
    (thumb_start, thumb_rows)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test gpui_shell::mouse:: 2>&1 | tail -30
```
Expected: 13 tests pass (10 from Tasks 4-5 + 3 new).

- [ ] **Step 5: Paint the scrollbar in `terminal_element.rs`'s `paint()`**

Add near the end of `paint()`, after the cursor block:

```rust
        // Scrollbar. 6px thumb on the right edge, matching the wgpu app's
        // build_scroll_bar_instances geometry.
        let (display_offset, history_size) = self.terminal.scrollback_info();
        let rows = self.terminal.rows as usize;
        let (thumb_start, thumb_rows) =
            mouse::scrollbar_thumb_geometry(rows, history_size, display_offset);
        if history_size > 0 {
            const SCROLLBAR_PX: Pixels = px(6.0);
            window.paint_quad(fill(
                Bounds {
                    origin: point(
                        bounds.origin.x + bounds.size.width - SCROLLBAR_PX,
                        bounds.origin.y + self.cell_height * thumb_start as f32,
                    ),
                    size: size(SCROLLBAR_PX, self.cell_height * thumb_rows as f32),
                },
                gpui::rgba(0xf8f8f260),
            ));
        }
```

- [ ] **Step 6: Add the scroll-wheel handler and thumb-drag detection to `mouse.rs`**

In `register_mouse_handlers`, add a new `on_mouse_event::<ScrollWheelEvent>` registration:

```rust
    let scroll_terminal = terminal.clone();
    window.on_mouse_event(move |event: &gpui::ScrollWheelEvent, phase, _window, _cx| {
        if phase != DispatchPhase::Bubble {
            return;
        }
        if !bounds.contains(&event.position) {
            return;
        }
        let pixel_delta = event.delta.pixel_delta(cell_height);
        let line_delta = (f32::from(pixel_delta.y) / f32::from(cell_height)).round() as i32;
        if line_delta != 0 {
            scroll_terminal.scroll_display(line_delta);
        }
    });
```

For thumb-drag: extend the mouse-down handler (from Tasks 4/5) with a check for "did this land in the scrollbar's 6px strip" *before* the mouse-report and selection checks, and track drag state (selecting text vs. dragging the thumb) so mouse-move routes to the right one. This needs a small addition to `ClickState`-adjacent state — add a `dragging_scrollbar: bool` field to a per-terminal drag-state map (parallel to `CLICK_STATE`, or folded into it) that mouse-down sets and mouse-up clears; mouse-move checks it to decide whether to call `update_selection` (existing, Task 4) or convert the new Y position to a `display_offset` and call `scroll_display`. Read Task 4's and Task 5's actual landed `register_mouse_handlers` code before writing this — this brief describes the shape (a boolean gate before existing logic, same pattern those two tasks used for mouse-report), not a byte-for-byte diff against code that will have moved on by the time this task starts.

- [ ] **Step 7: Build, fix errors, verify the full gate**

```bash
cargo build --bin gpui-petruterm 2>&1
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```

- [ ] **Step 8: Dogfood checkpoint — STOP and ask the user to confirm**

Ask the user to run `cargo run --bin gpui-petruterm` and:
1. Print enough output to scroll (`seq 1 200`), then scroll up with the mouse wheel and confirm history navigation works, and a visual scrollbar thumb appears on the right edge, sized/positioned correctly (near the top when scrolled far back, near the bottom when at the live edge).
2. Click-drag the scrollbar thumb itself and confirm it scrolls the view (not starting a text selection).
3. Confirm typing or new output still snaps the view back to the bottom (`scroll_to_bottom`, pre-existing behavior) — this task must not have broken that.
4. Confirm plain click-drag text selection (away from the scrollbar strip) still works exactly as after Tasks 4/5.

- [ ] **Step 9: Commit**

```bash
git add src/gpui_shell/terminal_element.rs src/gpui_shell/mouse.rs
git commit -m "[gpui-migration] feat: Scrollback scroll-wheel + visual scrollbar (M1b).

Scroll wheel converts gpui's ScrollWheelEvent delta to a line delta
and calls Terminal::scroll_display (already exists); new PTY output
or a keypress still snaps back via scroll_to_bottom (unchanged,
pre-existing behavior). Visual scrollbar thumb geometry
(scrollbar_thumb_geometry) is ported from
src/app/renderer/overlay.rs's build_scroll_bar_instances as-is: a
6px-wide thumb on the right edge, sized (screen_rows/total_lines)*
screen_rows and positioned at (1 - display_offset/history_size)*slack.
Drag-to-scroll on the thumb reuses the mouse-down/move architecture
Task 4 established, gated the same way Task 5's mouse-report check
was: a boolean check before the existing selection logic.

This completes M1b: ANSI colors, cursor shapes/blink, selection +
copy + mouse-report passthrough, and scrollback + scrollbar are all
implemented, tested where the logic is pure, and dogfooded.

scripts/ci-local.sh clean. Dogfooded: scroll wheel navigates history,
scrollbar renders and is draggable, auto-scroll-to-bottom still
works, plain text selection unaffected."
```

---

## M1b Exit Criteria

All four pieces are dogfood-confirmed working together: colored/styled output renders correctly, the cursor shows the right shape and blinks appropriately (including hollow-when-unfocused in split panes), click-drag/word/line selection with copy works and doesn't fire inside mouse-report-mode apps (vim, tmux), and scrollback navigates via both wheel and a draggable visual scrollbar. `scripts/ci-local.sh` passes clean after every task. `terminal_element.rs` is back under the project's 400-line module convention, with `font_state.rs`, `rasterize.rs`, and `mouse.rs` each owning one clear responsibility. M1b's completion unblocks M1c (emoji, LCD subpixel AA), not yet designed — brainstorm it when its turn comes.
