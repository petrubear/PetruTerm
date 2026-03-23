# Technical Debt Registry

**Last Updated:** 2026-03-23
**Total Items:** 7
**Critical (P0):** 0 | **P1:** 2 | **P2:** 3 | **P3:** 2

## Priority Definitions

| Priority | Definition | SLA |
|----------|------------|-----|
| P0 | Blocking development or causing incidents | Immediate |
| P1 | Significant impact on velocity or correctness | This sprint |
| P2 | Moderate impact, workaround exists | This quarter |
| P3 | Minor, address when convenient | Backlog |

---

## P0 - Critical

_None_

---

## P1 - High Priority

### TD-011: Shell `exit` does not close the terminal window
- **File:** `src/app.rs`, `src/term/pty.rs`
- **Issue:** When the shell process exits (`exit`, `Ctrl+D`), `PtyEvent::Exit` is
  received and logged, but `event_loop.exit()` (or pane/tab close) is never called.
  The window stays open with a dead PTY.
- **Impact:** Functional — user has to force-quit the app to close after `exit`.
- **Fix:** In `poll_pty_events`, set a flag (e.g. `needs_exit: bool`) when `Exit` is
  received. In `about_to_wait` or `RedrawRequested`, call `event_loop.exit()` when
  the flag is set. For multi-pane: close the pane/tab instead; only quit when the
  last pane exits.

### TD-002: PTY placeholder event proxy on Term construction
- **File:** `src/term/mod.rs`
- **Issue:** `Terminal::new()` constructs `Term<PtyEventProxy>` with a disconnected placeholder channel, then `Pty::spawn()` creates a *second* `PtyEventProxy` with the real channel. The placeholder proxy's events go nowhere.
- **Impact:** Low in practice (Term constructed once before spawn), but semantically wrong.
- **Fix:** Create the channel first, pass the same proxy to both `Term::new` and `Pty::spawn`. Requires splitting `Pty::spawn` to accept an existing proxy.

### ~~TD-003: PTY cell_width/cell_height hardcoded at 8×16~~ — RESOLVED

---

## P2 - Medium Priority

### TD-012: Nerd Font / special-character glyphs overflow cell bounds
- **File:** `src/font/shaper.rs`, `src/app.rs` (`build_instances`)
- **Issue:** Nerd Font PUA icons (U+E000–U+F8FF, U+F0000+) such as the Apple logo,
  git branch glyph, and Starship separator arrows render visibly taller/larger than
  regular characters. They overflow the cell height, clipping into adjacent rows.
- **Root cause:** Two likely causes (needs investigation):
  1. The swash rasterizer returns a bitmap larger than `cell_height` for these glyphs
     (their design is taller in the font metrics). `build_instances` uses the raw
     bitmap size as `glyph_size`, so the quad expands beyond the cell.
  2. `bearing_y` / ascent calculation pushes the glyph origin outside the cell rect.
- **Fix options:**
  1. Clamp `glyph_size` to `[cell_width, cell_height]` in `build_instances` so no
     glyph quad can exceed its cell.
  2. Scale oversized glyphs down proportionally to fit within the cell box.
  3. Add a scissor rect per cell in the render pass (GPU-level clipping).
- **Priority:** P2 — cosmetic but prominent with any Starship / Nerd Font theme.
- **Observed:** Apple logo, ⎇ branch icon, Starship separator arrows are taller than
  text rows; the prompt background segments look correct but icons bleed vertically.

### TD-004: Scrollback not verified at 100k lines
- **File:** `src/term/mod.rs`
- **Issue:** `scrolling_history: config.scrollback_lines` is set but not verified against the 100k line requirement.
- **Fix:** Test with `printf '%s\n' {1..110000}` and verify retention.

### TD-005: PTY thread JoinHandle type-erased
- **File:** `src/term/pty.rs`
- **Issue:** `EventLoop::spawn()` return value is boxed as `Box<dyn Any + Send>`. Thread can't be joined or inspected on exit.
- **Fix:** Add a `shutdown()` method that sends quit via notifier and waits.

### ~~TD-006: No mouse event handling~~ — RESOLVED

### ~~TD-007: No clipboard integration~~ — RESOLVED

### ~~TD-010: Nerd Font icons render as CJK fallback glyphs~~ — RESOLVED
- **File:** `src/font/loader.rs`, `src/font/shaper.rs`
- **Issue:** Starship prompt and other tools use Nerd Font Private Use Area codepoints (U+E000–U+F8FF, U+F0000+) for icons. The bundled JetBrains Mono does not include these glyphs. cosmic-text falls back to system CJK fonts for PUA codepoints, rendering Chinese characters instead of icons.
- **Observed:** File-type icons, git branch icon (󰘬), and arrow separators all render as CJK characters in the Starship prompt.
- **Impact:** Cosmetic — terminal is functional but prompt looks broken with any Nerd Font theme.
- **Fix options:**
  1. Bundle `JetBrainsMono Nerd Font` (the patched variant from nerdfonts.com) instead of stock JetBrains Mono.
  2. Bundle a dedicated Nerd Font symbols-only font (`NerdFontsSymbolsOnly`) as a fallback and load it after JetBrains Mono in fontdb — cosmic-text will try it for missing glyphs automatically.
  3. Instruct users to set a Nerd Font variant in `config.lua`: `font.family = "JetBrainsMono Nerd Font"`.
  - Option 2 is recommended: keeps the main font clean, symbols font is ~3MB, covers all PUA ranges.
- **Priority:** P2 — cosmetic, but prominent with any Nerd Font shell theme.

---

## P3 - Low Priority

### TD-008: Dead code / unused import warnings
- **Files:** `src/font/`, `src/renderer/`, `src/term/`, `src/ui/`
- **Issue:** ~23 warnings for unused stubs. Render loop being wired up has cleared most original offenders.
- **Fix:** Suppress with `#[allow(dead_code)]` on stub items that will be used in Phase 2/3.

### TD-009: Default config require() path not set up for embedded eval
- **File:** `src/config/mod.rs`
- **Issue:** `load_config_str` doesn't set `package.path`, so require() in embedded defaults fails. Currently mitigated because the user config is loaded from disk (which does set the path).
- **Fix:** Embed all default modules as flat Lua or eval them in sequence before the main config.

---

## Resolved Debt (Last 30 Days)

| ID | Title | Resolved | Resolution |
|----|-------|----------|------------|
| TD-003 | PTY cell_width/height hardcoded | 2026-03-23 | Pty::spawn/resize now accept cell_w/h from TextShaper; Terminal::resize propagated; WindowEvent::Resized now calls terminal.resize() |
| TD-007 | No clipboard integration | 2026-03-23 | arboard crate; Cmd+C copies selection, Cmd+V pastes (bracketed-paste aware); OSC 52 via PtyEvent::ClipboardStore/Load; PtyWrite forwarding |
| TD-006 | No mouse event handling | 2026-03-23 | CursorMoved/MouseInput/MouseWheel handled; drag selection via alacritty Selection API; SGR+X10 mouse reporting; scrollback scroll wheel |
| TD-001 | Cell rendering not connected | 2026-03-22 | Full pipeline wired: grid walk → shape_line → rasterize → CellVertex → bg+glyph draw passes |
| TD-010 | Nerd Font icons render as CJK | 2026-03-22 | Replaced bundled JetBrains Mono with JetBrains Mono Nerd Font Mono v3.3.0 |
| — | wgpu 29 API breaks | 2026-03-22 | `CurrentSurfaceTexture`, `TexelCopyTextureInfo`, `TexelCopyBufferLayout`, `immediate_size`, `multiview_mask`, `depth_slice` |
| — | alacritty_terminal SizeInfo removed | 2026-03-22 | Local `TermSize: Dimensions` trait implemented |
| — | mlua LuaError not Send+Sync | 2026-03-22 | Internal fns return `LuaResult<T>`, map at public boundary |
| — | cosmic-text AttrsList API | 2026-03-22 | `AttrsList::new(&attrs)`, `get_image_uncached()` for owned Option |
| — | Glyph mask sampled via wrong channel | 2026-03-22 | Changed mask RGBA storage to `[a,a,a,255]`; shader reads `.r` correctly |
| — | sRGB/linear color mismatch | 2026-03-22 | Switched surface format to non-sRGB; colors stored as sRGB display correctly |
| — | JetBrains Mono not installed | 2026-03-22 | Bundled via `include_bytes!` in `assets/fonts/`; always available |
| — | Retina HiDPI font too small | 2026-03-22 | `scale_factor` from `window.scale_factor()` applied to font size; cell size now 18×36px at 2× |
| — | Lua `require('petruterm')` fails | 2026-03-22 | Registered in `package.preload` after global injection |
