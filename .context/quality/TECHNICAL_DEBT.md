# Technical Debt Registry

**Last Updated:** 2026-03-30
**Total Items:** 21
**Critical (P0):** 0 | **P1:** 3 | **P2:** 3 | **P3:** 6

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

### ~~TD-028: Redundant Text Shaping (Performance)~~ — RESOLVED
- **File:** `src/app.rs` (`WindowEvent::RedrawRequested`)
- **Issue:** `shaper.shape_line` was called for every visible row on every frame (60+ times/sec), even if terminal content hadn't changed. Shaping (HarfBuzz) is expensive.
- **Fix:** Implemented a row-level `RowCache` in `App`. Rows are hashed (text + colors); cached shaped glyphs and GPU instances are reused if the hash matches.
- **WezTerm Inspiration:** WezTerm caches shaping results at the `Line` level (using `termwiz` appdata). It only re-shapes clusters when the underlying grid row is modified.

### ~~TD-029: $O(N^2)$ Column Calculation during Shaping (Performance)~~ — RESOLVED
- **File:** `src/font/shaper.rs` (`shape_line`)
- **Issue:** Column index for each glyph was calculated using `text[..start].chars().count()`. Inside a loop over all glyphs, this makes shaping a single line $O(N^2)$ relative to character count.
- **Fix:** `TextShaper::shape_line` now uses incremental character counts to determine glyph columns in $O(N)$.
- **WezTerm Inspiration:** WezTerm iterates through `CellCluster` objects and keeps an incremental count of the visual columns covered, avoiding redundant string traversals.

### ~~TD-030: Secret Leakage to LLM Provider~~ — RESOLVED
- **Implementation:** Added `sanitize_command` to `ShellContext`. Uses regex to redact `export VAR=secret` and Authorization headers from `last_command` before injecting into system prompt.
- **Result:** Sensitive credentials are no longer sent to the LLM provider in plaintext.

### ~~TD-031: Insecure API Key Storage~~ — RESOLVED
- **Implementation:** Switched `LlmConfig::api_key` to `secrecy::SecretString`. Added `#[serde(skip_serializing)]` to prevent keys from being written to disk or logs. Used `expose_secret()` only at the request boundary.
- **Result:** API keys are protected in memory and hidden from Debug/Serialization output.

### TD-032: High-Bandwidth GPU Instance Uploads (Performance)
- **File:** `src/renderer/gpu.rs` (`upload_instances`)
- **Issue:** The entire terminal grid (up to 32k instances, ~2MB) is uploaded to the GPU via `write_buffer` every frame, creating ~120MB/s of unnecessary memory traffic at 60 FPS.
- **Fix:** Use a "dirty-rect" or "dirty-row" approach to only upload modified `CellVertex` data, or use `wgpu` buffer mapping/staging to reduce copy overhead.
- **WezTerm Inspiration:** WezTerm caches the vertex data (quads) and only updates the GPU buffers for rows that have changed (dirty row tracking).

---

## P2 - Medium Priority

### ~~TD-033: Atlas Stability & Eviction (Stability)~~ — RESOLVED
- **File:** `src/renderer/atlas.rs`
- **Issue:** `GlyphAtlas` used a simple shelf-packer with no eviction policy. It would eventually fill up and crash/error if many unique glyphs (Nerd Fonts, different sizes) were rendered.
- **Fix:** Implemented a "flush and start over" strategy. `GlyphAtlas::upload` now returns `AtlasError::Full`. `App::render` catches this, clears both Glyph and LCD atlases, clears the `RowCache`, and re-renders the frame.
- **WezTerm Inspiration:** WezTerm uses a "flush and start over" strategy. When the atlas runs out of space (`OutOfTextureSpace`), it clears the entire atlas and re-populates it with just the glyphs needed for the current frame.

### TD-034: God Object Pattern in `App` (Architecture)
- **File:** `src/app.rs`
- **Issue:** `App` manages window lifecycle, GPU state, terminal instances, input mapping, AI logic, and UI rendering. It has too many responsibilities, making it hard to test and maintain.
- **Fix:** Refactor into specialized managers: `RenderContext` for GPU, `InputHandler` for keymaps, and `TabManager`/`PaneManager` for terminal layout.
- **WezTerm Inspiration:** WezTerm separates concerns into `TermWindow` (UI/Logic), `RenderState` (GPU), and a dedicated `Mux` (Multiplexer) for managing terminals and tabs.

### TD-035: Tight Coupling between UI and Terminal (Architecture)
- **File:** `src/app.rs`, `src/ui/`
- **Issue:** `App` manually iterates over panes and terminals for resizing and event polling. The UI layout logic is not sufficiently isolated from the terminal state.
- **Fix:** Define a clear trait-based interface for UI components to interact with terminal instances, allowing for easier testing and alternative UI implementations.
- **WezTerm Inspiration:** WezTerm uses a decoupled model where the terminal state (`Pane`) is distinct from the windowing layer, communicating via events and shared state.

### TD-036: Suboptimal Render Pass Architecture (Performance)
- **File:** `src/renderer/gpu.rs`
- **Issue:** Rendering uses three separate passes (BG, Glyph, LCD). On Tiled Deferred GPUs (macOS/iOS), multiple passes with `LoadOp::Load` force unnecessary tile memory reloads.
- **Fix:** Consolidate BG and Glyph passes into a single render pass if possible, or optimize load/store operations to minimize bandwidth.
- **WezTerm Inspiration:** WezTerm's OpenGL/WebGPU renderers minimize passes. Its WebGPU backend leverages modern bind-groups to draw backgrounds and glyphs in a more unified flow.

---

## P3 - Low Priority

### TD-037: Incomplete Palette Actions (Implementation)
- **File:** `src/app.rs` (`handle_palette_action`)
- **Issue:** "Explain Last Output" and "Fix Last Error" in the command palette are currently stubs that just log info, despite implementations existing as methods in `App`.
- **Fix:** Wire up the palette actions to call `explain_last_output()` and `fix_last_error()`.

### TD-038: Hardcoded UI Constants (Implementation)
- **File:** `src/app.rs` (`build_chat_panel_instances`)
- **Issue:** AI panel colors, paddings, and layout constants are hardcoded in the rendering function.
- **Fix:** Move UI constants to the Lua configuration system or a dedicated `theme.lua` module.
- **WezTerm Inspiration:** Virtually all UI aspects in WezTerm are configurable via Lua, from border widths to individual color palette indices.

### TD-039: Manual ANSI Key Encoding (Implementation)
- **File:** `src/app.rs` (`send_key_to_active_terminal`)
- **Issue:** Arrow keys and other special keys are manually converted to ANSI escape sequences. This is error-prone and hard to extend.
- **Fix:** Use a dedicated key-to-sequence mapping library or a data-driven approach based on the `TERM` definition.
- **WezTerm Inspiration:** WezTerm uses a robust input mapping system that translates `winit` events into terminal sequences based on the current terminal mode and `TERM` capability database.

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

### TD-005: PTY thread JoinHandle type-erased
- **File:** `src/term/pty.rs`
- **Issue:** `EventLoop::spawn()` return value is boxed as `Box<dyn Any + Send>`. Thread can't be joined or inspected on exit.
- **Fix:** Add a `shutdown()` method that sends quit via notifier and waits.

### ~~TD-006: No mouse event handling~~ — RESOLVED

### ~~TD-007: No clipboard integration~~ — RESOLVED

### ~~TD-010: Nerd Font icons render as CJK fallback glyphs~~ — RESOLVED

---

## Resolved Debt (Last 30 Days)

| ID | Title | Resolved | Resolution |
|----|-------|----------|------------|
| TD-028 | Redundant Text Shaping | 2026-03-30 | Row-level caching (RowCache) with hashing. |
| TD-029 | O(N^2) Column Calculation | 2026-03-30 | Incremental column tracking in shape_line. |
| TD-033 | Atlas Stability & Eviction | 2026-03-30 | Flush-and-restart strategy on AtlasError::Full. |
| TD-025 | Vertical spacing too tight | 2026-03-27 | font.line_height config (default 1.2). |
| TD-018 | Powerline separator fringing | 2026-03-27 | Premultiplied alpha + blend: One/OneMinusSrcAlpha. |
| TD-012 | Nerd Font icons overflow cell | 2026-03-23 | clamp_glyph_to_cell() crops glyph_size. |
