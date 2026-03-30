# Technical Debt Registry

**Last Updated:** 2026-03-30
**Total Items:** 9
**Critical (P0):** 0 | **P1:** 0 | **P2:** 2 | **P3:** 6

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

_None_

### ~~TD-021: Drag-and-drop file path not inserted~~ — RESOLVED
- `WindowEvent::DroppedFile`: panel focused → append to chat input; terminal focused → write path to PTY.

### ~~TD-019: Space key not forwarded in AI block input~~ — RESOLVED
- Explicit `Key::Named(NamedKey::Space)` handler in panel input routing.

### ~~TD-020: AI block response not rendered~~ — RESOLVED
- `build_chat_panel_instances` rewritten from scratch; `push_shaped_row` helper; panel rendered to the right of terminal at `col_offset = term_cols`.

### ~~TD-016: Ctrl key modifier not forwarded to PTY~~ — RESOLVED (commit d70c00d)

### ~~TD-017: Reverse-video (SGR 7 / Flags::INVERSE) not applied in cell rendering~~ — RESOLVED (commit d70c00d)

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

### ~~TD-024: Mouse text selection not working~~ — RESOLVED
- `cell_in_selection()` checks `SelectionRange` per cell; selected cells rendered with inverted fg/bg.
- `start_selection` guarded by `!any_mouse` (no conflict with nvim/tmux mouse reporting).
- Window drag: `setMovableByWindowBackground: NO`; clicks in pad_top zone → `window.drag_window()`.

### ~~TD-018: catppuccin tmux separators don't blend with adjacent cells~~ — RESOLVED
<!--
Root cause: fragment shader was doing mix(bg, fg, alpha) and returning alpha=1.0 always.
Transparent edge pixels of powerline glyphs wrote the separator's bg color over the adjacent
cell's background, creating a visible fringe strip.
Fix (2026-03-27):
- Shader switched to premultiplied alpha: returns vec4(fg_srgb * alpha, alpha) instead of mix.
- wgpu blend state: SrcAlpha → One (matches premultiplied output, One/OneMinusSrcAlpha).
  Alpha-0 glyph pixels are now fully transparent; bg pass colour shows through correctly.
Note: right-edge clamping was initially added but later removed (2026-03-27) because it broke
double-wide Nerd Font icons (MonoLisa NF non-Mono). Premultiplied alpha alone is sufficient
to fix the fringing — overflowing transparent pixels cause no visible artifact.
-->

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

### TD-027: Powerline separator arrows render less vivid than WezTerm
- **File:** `src/renderer/pipeline.rs` (`fs_main`), `src/font/shaper.rs`
- **Issue:** Powerline separator arrows (U+E0B0 etc.) appear darker / less saturated than WezTerm. Root cause is twofold: (1) swash greyscale AA does not produce alpha=1.0 for solid-fill pixels the way CoreText does; (2) the linear-space bg-aware premultiplied blend in `fs_main` partially compensates but edge-pixel contribution is still attenuated by the `ca * ca` double-weight.
- **Current state (2026-03-30):** `fs_main` uses hybrid bg-aware premul: `mix(bg_lin, fg_lin, ca)` in linear space, output `(rgb * ca, ca)`. Solid-fill pixels → `ca=1 → fg_srgb` (vivid). Edge pixels attenuated more than pure bg_aware (9% fg vs 30% fg at ca=0.3). Overall better than plain premul but still slightly duller than WezTerm.
- **Attempts so far:** (1) bg_aware REPLACE — vivid but had TD-018 fringing; (2) premul sRGB — no fringing but dark; (3) hybrid bg-aware premul — best compromise so far, still not matching WezTerm.
- **Next steps to investigate:**
  1. Verify swash `SwashContent::Mask` alpha for solid pixels — add debug log for max alpha per glyph. If max < 230, swash quality is the bottleneck (not shader math).
  2. Try `pow(alpha, 1/2.2)` (standard gamma) instead of `1/1.4` — might push solid pixels closer to 1.0.
  3. Consider rendering powerline glyphs via CoreText directly on macOS (use `objc2` + `CGFont`) for solid fill — these glyphs don't benefit from LCD AA and just need solid coverage.
  4. WezTerm approach: render bg-pass to a separate texture, bind it as `@group(2)` in the glyph pass — then `fs_main` samples the ACTUAL background at each fragment instead of using vertex-bg, eliminating the premul attenaution entirely.
- **Priority:** P3 — text and regular glyphs look correct; only Starship/Nerd Font separators are affected.

### TD-022: Chat panel has no access to current working directory or project files
- **File:** `src/llm/` (new), `src/app.rs`
- **Issue:** The chat panel sends user messages to the LLM with only a static system prompt. It has no awareness of the current working directory, open files, shell history, or directory listing. This limits the AI's usefulness for project-specific questions ("explain this file", "what's in this directory", "why did that command fail").
- **Vision:** Implement a lightweight local agent that runs when the chat panel is open. The agent would: (1) capture CWD from the PTY via OSC sequences or shell integration hooks; (2) on user query, attach relevant context (CWD, `ls` output, relevant file snippets) to the system prompt; (3) support tool calls: `read_file`, `list_dir`, `run_command` — executed locally in a sandboxed manner; (4) multi-turn with tool results fed back to the model.
- **Scope:** Substantial — requires shell integration script (TD roadmap item), a tool-call loop in the tokio task, and a context-assembly step before each LLM call. Warrants its own design doc before implementation.
- **Priority:** P3 — chat works for general questions; agent mode is a Phase 3 feature.

### TD-023: Leader key for panel and pane actions
- **Files:** `src/app.rs`, `src/config/schema.rs`
- **Issue:** Panel toggle (Ctrl+C) and focus switch (Ctrl+V) conflict with standard terminal shortcuts (SIGINT, literal-next). As more panel actions are added (explain output, fix error, run last command), each will need a dedicated keybind — and the available Ctrl+key space is nearly exhausted by terminal conventions.
- **Vision:** Implement a second leader key (separate from the tmux leader used for pane splits) dedicated to AI/panel actions. Example: `Ctrl+A` as default. Sequence: `Ctrl+A` → panel opens if closed, then next key selects action:
  - `Ctrl+A` again → close panel
  - `Tab` / `Ctrl+V` → switch focus
  - `e` → explain last output (Ctrl+Shift+E)
  - `f` → fix last error (Ctrl+Shift+F)
  - `r` → run last AI command
  This mirrors how tmux solves the same conflict — all multiplexer actions live behind a prefix, leaving the raw key space to the running process.
- **Migration:** Once implemented, Ctrl+C/Ctrl+V panel bindings become aliases or are removed.
- **Priority:** P3 — current bindings work; this is a polish/extensibility improvement.

### TD-026: Glyph antialiasing quality vs WezTerm
- **File:** `src/renderer/atlas.rs`, `src/font/shaper.rs`, fragment shader in `src/renderer/pipeline.rs`
- **Issue:** Rendered text looks rougher / less crisp than WezTerm at the same font size and DPI. WezTerm applies subpixel antialiasing (freetype LCD rendering or CoreText CG subpixel AA on macOS) and composites glyphs against the actual cell background colour, whereas PetruTerm rasterises masks in greyscale and blends in the fragment shader without background-aware correction.
- **Investigation:** Review WezTerm's antialiasing pipeline:
  - `wezterm-font/src/rasterizer/` — how it selects between freetype, CoreText, and DirectWrite backends per platform.
  - `wezterm-render/` — how LCD RGB masks (3-channel) are stored in the atlas and composited against bg in the shader (separate R/G/B coverage → per-channel lerp).
  - Check whether swash exposes subpixel/LCD output or only greyscale masks, and whether cosmic-text passes through subpixel hints.
- **Reference:** WezTerm `wezterm-font/src/rasterizer/freetype.rs` (LCD filter flags) and `wezterm-render/src/glyphcache.rs` (atlas format + shader).
- **Priority:** P3 — text is readable; this is a polish/fidelity improvement for Phase 2.

**Resolution: 3-level plan (in progress)**

### TD-026a: Greyscale gamma correction (Nivel 1)
- **Status:** DONE (2026-03-30)
- **Fix:** Use existing `srgb_to_lin`/`lin_to_srgb` helpers in fragment shader. Convert fg to linear, blend with bg (linear), convert back to sRGB. Applies `pow(alpha, 1/2.2)` for greyscale gamma.
- **Effort:** Low — shader change only.
- **Commit:** shader pipeline.rs:84-93 — `fs_main` now converts fg sRGB→linear, applies `pow(alpha, 1/2.2)`, outputs premultiplied linear for GPU blend.

### TD-026b: Background-aware blending (Nivel 2)
- **Status:** DONE (2026-03-30)
- **Fix:** `fs_bg_aware` does gamma-correct blend manually in linear space: sRGB→linear for both fg and bg, `mix(bg_lin, fg_lin, pow(alpha, 1/2.2))`, then linear→sRGB output with alpha=1.0. Separate `CellPipelineBgAware` pipeline with `BlendState::REPLACE` (no GPU premultiplied blend).
- **Effort:** Medium — requires pipeline re-evaluation.
- **Commit:** `fs_bg_aware` shader + `CellPipelineBgAware` pipeline in `src/renderer/pipeline.rs`; `GpuRenderer` uses it for glyph pass.

### TD-026c: LCD subpixel AA (Nivel 3)
- **Status:** DONE (2026-03-30) — FULLY WIRED
- **Fix:** FreeType LCD rasterizer (`FreeTypeLcdRasterizer`) using `FT_RENDER_MODE_LCD`; deinterleaves R/G/B subpixel data into RGBA atlas format. Separate `LcdGlyphAtlas` + `CellPipelineLcd` shader with `fs_lcd` entry point that blends per-channel in linear space against cell background. LCD glyphs stored in separate atlas from SwashCache greyscale glyphs. LCD AA enabled via `config.font.lcd_antialiasing = true`.
- **Architecture:** `GpuRenderer` owns `LcdGlyphAtlas` (via `RefCell`), `TextShaper` uses `FreeTypeLcdRasterizer` which writes to that atlas. Glyphs with `FLAG_LCD` go through separate `lcd_instances` buffer and `upload_lcd_instances()`. Render pass 3 (LCD pass) draws LCD glyphs with `fs_lcd` shader reading from LCD atlas bind group (`@group(2)`).
- **Effort:** High — major architecture change.
- **Commit:** `src/font/freetype_lcd.rs` (FreeType LCD rasterizer), `src/renderer/lcd_atlas.rs` (LCD atlas), `src/renderer/pipeline.rs` (LCD shader + `CellPipelineLcd`), `src/renderer/gpu.rs` (LCD infrastructure wired), `src/font/shaper.rs` (`rasterize_lcd_to_atlas`, `ch` field added to `ShapedGlyph`), `src/app.rs` (LCD instance routing in `build_instances`), `Cargo.toml` (added `freetype = "0.7"`).

### ~~TD-025: Vertical spacing between terminal lines too tight~~ — RESOLVED
<!--
Fix: `font.line_height: f32` added to `FontConfig` (default 1.2); `TextShaper::new` passes
`font_config.size * font_config.line_height` as line_height to `cosmic_text::Metrics`.
`measure_cell` reads `run.line_height` which reflects the multiplier → `cell_height` propagates
to PTY resize via `cell_dims()`. Configurable from Lua: `font.line_height = 1.4`.
-->

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
| TD-025 | Vertical spacing too tight | 2026-03-27 | `font.line_height: f32` (default 1.2) in FontConfig; Metrics line_height = size * multiplier; propagates to cell_height + PTY |
| TD-018 | Powerline separator colour fringing | 2026-03-27 | Premultiplied alpha in shader + blend state (One/OneMinusSrcAlpha); glyph right-edge clamped to cell_width |
| — | Ligature rendering (negative bearing_x clipped) | 2026-03-27 | Removed X-axis clamping in build_instances; bearing_x passed raw to shader |
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
