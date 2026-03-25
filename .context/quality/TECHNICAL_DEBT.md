# Technical Debt Registry

**Last Updated:** 2026-03-24
**Total Items:** 4
**Critical (P0):** 0 | **P1:** 0 | **P2:** 2 | **P3:** 2

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

### ~~TD-016: Ctrl key modifier not forwarded to PTY~~ — RESOLVED

### ~~TD-017: Reverse-video (SGR 7 / Flags::INVERSE) not applied in cell rendering~~ — RESOLVED

### TD-016: Ctrl key modifier not forwarded to PTY
- **File:** `src/app.rs` (`send_key_to_active_terminal`)
- **Issue:** Ctrl+letter combinations (Ctrl+U=erase line, Ctrl+A=go to start, Ctrl+C=interrupt,
  Ctrl+L=clear, etc.) are silently dropped. The key match arm `Key::Character(s)` sends the raw
  character `s` without checking `ctrl` modifier. On macOS, Ctrl+A generates `Key::Character("a")`
  with modifier state `ctrl=true`, not a control byte.
- **Root cause:** `send_key_to_active_terminal` reads `self.modifiers.state().control_key()` only
  to detect the tmux leader key (Ctrl+B). Printable keys always send the literal character.
- **Fix:** When `ctrl=true` and key is an ASCII letter/`[`, `\`, `]`, `^`, `_`, encode as
  control byte: `(char as u8) & 0x1F`. E.g. `Ctrl+A` → `\x01`, `Ctrl+U` → `\x15`.
  Also forward `Ctrl+2`=`\x00`, `Ctrl+3`=`\x1b`, `Ctrl+4`=`\x1c`, etc.
- **Priority:** P1 — blocks core terminal usage (line editing, tmux prefix, vim commands).

### TD-017: Reverse-video (SGR 7 / Flags::INVERSE) not applied in cell rendering
- **File:** `src/app.rs` (`collect_grid_cells`)
- **Issue:** Cells with `Flags::INVERSE` set (reverse-video attribute, SGR 7) render with
  declared fg/bg rather than swapped colors. `collect_grid_cells` uses `(cell.fg, cell.bg)`
  directly without checking `cell.flags.contains(Flags::INVERSE)`.
- **Context:** This was identified and fixed in a stale branch commit (`de62cae`) that was
  lost during a git rebase. The fix is one-liner in `collect_grid_cells`:
  ```rust
  let (fg, bg) = if cell.flags.contains(Flags::INVERSE) {
      (cell.bg, cell.fg)
  } else {
      (cell.fg, cell.bg)
  };
  ```
- **Impact:** All programs using reverse-video show wrong colors: `less`, `man`, vim status bar,
  tmux status line, and catppuccin theme separators. This is blocking the tmux/nvim smoke tests.
- **Priority:** P1 — visually broken for any TUI app using reverse-video.

### ~~TD-011: Shell `exit` does not close the terminal window~~ — RESOLVED

### ~~TD-013: Arrow keys ignore APP_CURSOR mode (DECCKM)~~ — RESOLVED

### ~~TD-002: PTY placeholder event proxy on Term construction~~ — RESOLVED

### ~~TD-003: PTY cell_width/cell_height hardcoded at 8×16~~ — RESOLVED

---

## P2 - Medium Priority

### ~~TD-012: Nerd Font / special-character glyphs overflow cell bounds~~ — RESOLVED
<!--
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
-->

### ~~TD-004: Scrollback not verified at 100k lines~~ — RESOLVED
- Scrollback rendering fixed (display_offset applied); 110k lines confirmed scrollable.

### TD-018: catppuccin tmux separators don't blend with adjacent cells
- **File:** `src/app.rs` (`collect_grid_cells`, `build_instances`)
- **Issue:** catppuccin-tmux uses powerline separator glyphs (U+E0B0 ``, U+E0B2 ``,
  U+E0B4 ``, U+E0B6 `` and their sub-variants U+E0B1/E0B3) to draw pill/arrow shapes.
  These glyphs work by using the FOREGROUND color of the separator cell to match the
  BACKGROUND color of the adjacent cell, creating a seamless color-blend appearance.
  The cell containing the separator has: fg = color of adjacent segment, bg = current segment.
  When `Flags::INVERSE` is also set (TD-017), the colors are additionally swapped.
  Currently these separators render with incorrect colors because:
  1. TD-017 (INVERSE not applied) causes wrong fg/bg on the separator cells themselves.
  2. The glyph is rendered on top of a solid-color background quad — if the background of the
     separator cell does not match the adjacent cell's background, the "blending" looks wrong
     regardless of color correctness.
- **Note:** The blending effect is purely a fg/bg color rendering concern — no GPU alpha
  blending is needed. Fixing TD-017 first will resolve most of this. The remainder is a
  color-assignment issue in catppuccin's tmux config that expects correct reverse-video handling.
- **Priority:** P2 — cosmetic but prominent with catppuccin-tmux; also affects tmux smoke test.

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
| TD-004 | Scrollback rendering broken (display_offset ignored) | 2026-03-24 | `collect_grid_cells` uses `Line(row - display_offset)`; trackpad PixelDelta divisor fixed (logical pts) |
| TD-013 | Arrow keys ignore APP_CURSOR (DECCKM) | 2026-03-24 | `send_key_to_active_terminal` reads `TermMode::APP_CURSOR`; sends `\x1bO_` vs `\x1b[_` |
| TD-002 | PTY placeholder proxy drops PtyWrite | 2026-03-24 | `Arc<OnceLock<Notifier>>` shared between Term proxy and Pty::spawn; atuin/TUI cursor queries now work |
| TD-012 | Nerd Font icons overflow cell | 2026-03-23 | clamp_glyph_to_cell() crops glyph_size + atlas_uv to cell bounds before emitting CellVertex |
| TD-011 | exit doesn't close window | 2026-03-23 | poll_pty_events returns (has_data, shell_exited); both about_to_wait and RedrawRequested call event_loop.exit() on exit |
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
