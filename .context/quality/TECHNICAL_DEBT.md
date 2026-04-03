# Technical Debt Registry

**Last Updated:** 2026-04-03
**Total Items:** 23
**Critical (P0):** 0 | **P1:** 0 | **P2:** 1 | **P3:** 0

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
- **Implementation:** Implemented a row-level `RowCache` in `App`. Rows are hashed (text + colors); cached shaped glyphs and GPU instances are reused if the hash matches.
- **Result:** Drastic reduction in CPU usage when terminal content is static.

### ~~TD-029: $O(N^2)$ Column Calculation during Shaping (Performance)~~ — RESOLVED
- **Implementation:** `TextShaper::shape_line` uses incremental character counts to determine glyph columns in $O(N)$.
- **Result:** Faster shaping for long lines.

### ~~TD-030: Secret Leakage to LLM Provider~~ — RESOLVED
- **Implementation:** Added `sanitize_command` to `ShellContext`. Uses regex to redact `export VAR=secret` and Authorization headers from `last_command` before injecting into system prompt.
- **Result:** Sensitive credentials are no longer sent to the LLM provider in plaintext.

### ~~TD-031: Insecure API Key Storage~~ — RESOLVED
- **Implementation:** Switched `LlmConfig::api_key` to `secrecy::SecretString`. Added `#[serde(skip_serializing)]` to prevent keys from being written to disk or logs. Used `expose_secret()` only at the request boundary.
- **Result:** API keys are protected in memory and hidden from Debug/Serialization output.

### ~~TD-032: High-Bandwidth GPU Instance Uploads~~ — RESOLVED
- **Implementation:** Added dirty-row tracking to `RowCache`. `GpuRenderer::upload_instances` now supports partial buffer updates via offset. `App::RedrawRequested` only uploads rows that were modified (cache misses) since the last frame.
- **Result:** Drastic reduction in GPU memory bandwidth (only changed rows are uploaded instead of 2MB every frame).

---

## P2 - Medium Priority

### ~~TD-033: Atlas Stability & Eviction (Stability)~~ — RESOLVED
- **Implementation:** Implemented a "flush and start over" strategy. `GlyphAtlas::upload` now returns `AtlasError::Full`. `App::render` catches this, clears both Glyph and LCD atlases, clears the `RowCache`, and re-renders the frame.
- **Result:** Terminal no longer crashes or stops rendering when the atlas fills up.

### ~~TD-034: God Object Pattern in `App` (Architecture)~~ — RESOLVED
- **Implementation:** Decomposed the 2000-line `App` struct into specialized managers: `RenderContext` (GPU), `Mux` (PTY/Tabs/Panes), `UiManager` (AI/Overlays), and `InputHandler` (Keyboard/Mouse).
- **Result:** Drastic improvement in maintainability and modularity. `App` is now a thin event coordinator.

### ~~TD-040: Leader Key Action Dispatch System (UX / Architecture)~~ — RESOLVED
- **Files:** `src/app/input/mod.rs`, `src/config/schema.rs`, `src/config/lua.rs`, `src/ui/palette/actions.rs`, `config/default/keybinds.lua`
- **Resolution:** `InputHandler` builds a `leader_map: HashMap<String, Action>` at startup from `config.keys`. All custom keybinds declared in `keybinds.lua` as `{ mods = "LEADER", key = "…", action = petruterm.action.… }`. Hardcoded `Cmd+Shift+P/A`, `Ctrl+Shift+E/F`, `Cmd+T/W` removed; replaced by leader bindings. `Action::FromStr` maps string names to enum variants. Adding a new binding requires only a Lua change.

---

#### ~~Background — What Already Exists~~

The infrastructure is mostly in place; it just needs to be wired together.

1. **`LeaderConfig`** (`src/config/schema.rs` line ~197):
   ```rust
   pub struct LeaderConfig {
       pub key: String,       // e.g. "b"
       pub mods: String,      // e.g. "CTRL"
       pub timeout_ms: u64,
   }
   ```

2. **`InputHandler`** (`src/app/input/mod.rs`) already tracks leader state:
   ```rust
   pub leader_active: bool,
   pub leader_timer: Option<Instant>,
   pub leader_timeout_ms: u64,
   ```
   And the dispatch block (lines ~228–243) fires after a leader keypress:
   ```rust
   if self.leader_active {
       self.leader_active = false;
       self.leader_timer = None;
       if let Key::Character(s) = &event.logical_key {
           match s.as_str() {
               "%" => { /* split horizontal */ }
               "\"" => { /* split vertical */ }
               "x" => { mux.cmd_close_pane(); }
               _ => {}
           }
       }
       return;
   }
   ```
   **Problem:** actions are hardcoded here; no Lua config is consulted.

3. **`Action` enum** (`src/ui/palette/actions.rs`) already lists all available actions:
   ```rust
   pub enum Action { NewTab, CloseTab, SplitHorizontal, SplitVertical, ClosePane,
                     ToggleAiMode, ExplainLastOutput, FixLastError, CommandPalette, … }
   ```
   `UiManager::handle_palette_action` already knows how to execute all of them.

4. **`keybinds.lua`** (`config/default/keybinds.lua`) already declares leader-bound keys:
   ```lua
   { mods = "LEADER", key = "%",  action = petruterm.action.SplitHorizontal },
   { mods = "LEADER", key = '"',  action = petruterm.action.SplitVertical },
   { mods = "LEADER", key = "x",  action = petruterm.action.ClosePane },
   ```
   **Problem:** the Lua parser does NOT read `config.keys`; these entries are purely decorative.

5. **`Action::ToggleAiMode`** exists but there is no `Action::ToggleAiPanel`. They can be unified or `ToggleAiMode` repurposed — it already opens the panel and focuses it (see `ui.rs` line ~183).

---

#### Implementation Plan

##### Step 1 — Add `keys: Vec<KeyBind>` to `Config`

In `src/config/schema.rs`, add:

```rust
/// A single keybind entry parsed from Lua's `config.keys` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBind {
    pub mods:   String,  // "LEADER", "CMD", "CMD|SHIFT", "CTRL|SHIFT", …
    pub key:    String,  // "a", "%", "p", …
    pub action: String,  // action name as string, resolved at load time
}
```

Add `pub keys: Vec<KeyBind>` to `Config`, with `Default` returning `vec![]`.

##### Step 2 — Parse `config.keys` in the Lua loader

In `src/config/lua.rs`, inside the function that builds a `Config` from the Lua VM (look for the section that reads `config.leader`), add:

```rust
if let Ok(keys_table) = lua_config.get::<mlua::Table>("keys") {
    for pair in keys_table.sequence_values::<mlua::Table>() {
        if let Ok(entry) = pair {
            let mods:   String = entry.get("mods").unwrap_or_default();
            let key:    String = entry.get("key").unwrap_or_default();
            let action: String = entry.get("action").unwrap_or_default();
            config.keys.push(KeyBind { mods, key, action });
        }
    }
}
```

Note: `petruterm.action.SplitHorizontal` in Lua should resolve to the string `"SplitHorizontal"`. Implement `petruterm.action` as a simple Lua table of string constants in the Lua prelude (in `src/config/lua.rs` where the `petruterm` module is registered):

```lua
petruterm.action = {
    SplitHorizontal  = "SplitHorizontal",
    SplitVertical    = "SplitVertical",
    ClosePane        = "ClosePane",
    NewTab           = "NewTab",
    CloseTab         = "CloseTab",
    CommandPalette   = "CommandPalette",
    ToggleAiPanel    = "ToggleAiPanel",
    ExplainLastOutput= "ExplainLastOutput",
    FixLastError     = "FixLastError",
    Quit             = "Quit",
}
```

##### Step 3 — Build a leader map at startup

In `src/app/input/mod.rs`, add a `leader_map` field to `InputHandler`:

```rust
pub leader_map: std::collections::HashMap<String, crate::ui::palette::Action>,
```

In `InputHandler::new`, build the map from `config.keys`:

```rust
let leader_map = config.keys.iter()
    .filter(|kb| kb.mods.to_ascii_uppercase() == "LEADER")
    .filter_map(|kb| {
        let action = Action::from_str(&kb.action).ok()?;
        Some((kb.key.clone(), action))
    })
    .collect();
```

This requires `Action` to implement `FromStr`. Add it in `src/ui/palette/actions.rs`:

```rust
impl std::str::FromStr for Action {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "SplitHorizontal"   => Ok(Action::SplitHorizontal),
            "SplitVertical"     => Ok(Action::SplitVertical),
            "ClosePane"         => Ok(Action::ClosePane),
            "NewTab"            => Ok(Action::NewTab),
            "CloseTab"          => Ok(Action::CloseTab),
            "CommandPalette"    => Ok(Action::CommandPalette),
            "ToggleAiPanel"     => Ok(Action::ToggleAiMode),  // alias
            "ExplainLastOutput" => Ok(Action::ExplainLastOutput),
            "FixLastError"      => Ok(Action::FixLastError),
            "Quit"              => Ok(Action::Quit),
            _                   => Err(()),
        }
    }
}
```

Also add `Action::ToggleAiPanel` as a proper variant (or keep the alias above — either works).

##### Step 4 — Replace the hardcoded leader dispatch

In `src/app/input/mod.rs`, replace the hardcoded `match s.as_str()` block inside `if self.leader_active { … }` with a lookup:

```rust
if self.leader_active {
    self.leader_active = false;
    self.leader_timer = None;

    if let Key::Character(s) = &event.logical_key {
        let key_str = s.to_ascii_lowercase();
        if let Some(action) = self.leader_map.get(key_str.as_str()).cloned() {
            let rc = render_ctx.as_mut().expect("RenderContext");
            let mut cfg_temp = config.clone();
            ui.handle_palette_action(action, mux, rc, &mut cfg_temp, window, wakeup_proxy);
        }
    }
    return;
}
```

`handle_palette_action` (in `src/app/ui.rs`) already handles every action including
`SplitHorizontal`, `ClosePane`, `ToggleAiMode`, etc., so no new action routing is needed.

##### Step 5 — Update `keybinds.lua`

Add the new AI panel binding and move the existing leader bindings so they are the
single source of truth:

```lua
-- AI panel
{ mods = "LEADER", key = "a", action = petruterm.action.ToggleAiPanel },

-- Pane management (already present — no change needed if Step 2 is correct)
{ mods = "LEADER", key = "%",  action = petruterm.action.SplitHorizontal },
{ mods = "LEADER", key = '"',  action = petruterm.action.SplitVertical },
{ mods = "LEADER", key = "x",  action = petruterm.action.ClosePane },
```

##### Step 6 — Wire `Config` into `InputHandler::new`

`InputHandler::new` currently takes only `leader_timeout_ms: u64`. Change it to accept
the full `Config` (or a `&[KeyBind]` slice) so it can build `leader_map`:

```rust
// Before
pub fn new(leader_timeout_ms: u64) -> Self

// After
pub fn new(config: &Config) -> Self
```

Update the call site in `src/app/mod.rs`:

```rust
// Before
input: InputHandler::new(config.leader.timeout_ms),

// After
input: InputHandler::new(&config),
```

---

#### Acceptance Criteria

- `<leader>a` opens/focuses/closes the AI panel (same cycle as `Cmd+Shift+A`).
- `<leader>%`, `<leader>"`, `<leader>x` continue to work as before (now via map, not hardcoded).
- Adding any new leader binding requires only a Lua change — no Rust recompile.
- Unknown leader keys are silently ignored (no crash).
- `cargo check` passes with 0 errors.

---

### TD-035: Tight Coupling between UI and Terminal (Architecture)
- **File:** `src/app.rs`, `src/ui/`
- **Issue:** `App` manually iterates over panes and terminals for resizing and event polling. The UI layout logic is not sufficiently isolated from the terminal state.
- **Fix:** Define a clear trait-based interface for UI components to interact with terminal instances, allowing for easier testing and alternative UI implementations.
- **WezTerm Inspiration:** WezTerm uses a decoupled model where the terminal state (`Pane`) is distinct from the windowing layer, communicating via events and shared state.

### ~~TD-036: Suboptimal Render Pass Architecture~~ — RESOLVED
- **Implementation:** Consolidated "BG pass" and "Glyph pass" into a single render pass ("terminal pass"). Leveraging premultiplied alpha in the glyph shader, we can draw backgrounds and then glyphs sequentially in the same encoder without reloading tile memory from VRAM.
- **Result:** Improved GPU efficiency and reduced power consumption, especially on Apple Silicon.

---

## P3 - Low Priority

### ~~TD-037: Incomplete Palette Actions~~ — RESOLVED
- **Implementation:** Connected `Action::ExplainLastOutput` and `Action::FixLastError` in `handle_palette_action` to their respective methods in `App`.
- **Result:** Command palette now correctly triggers AI context analysis.

### ~~TD-038: Hardcoded UI Constants~~ — RESOLVED
- **Implementation:** Introduced `ChatUiConfig` in the schema. Moved hardcoded colors and panel width from `src/app.rs` to the Lua configuration system (`llm.ui`). Added `parse_hex_linear` helper to support hex strings in Lua.
- **Result:** AI panel appearance is now fully customizable via Lua.

### ~~TD-039: Manual ANSI Key Encoding~~ — RESOLVED
- **Implementation:** Created `src/app/input/key_map.rs` with a structured `translate_key` function. Supports xterm-compatible modifier encoding (Shift, Ctrl, Alt) for Arrows, F-keys, and navigation keys.
- **Result:** Robust and extensible input handling that follows industry standards.
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

### ~~TD-005: PTY thread JoinHandle type-erased~~ — RESOLVED
- **Implementation:** Replaced type-erased `Box` with `std::thread::JoinHandle<()>`. Added a `shutdown()` method to `Pty` that sends `Msg::Shutdown` to the event loop and joins the thread. `App` now implements `Drop` to ensure all PTYs are shut down cleanly on exit.
- **Result:** No more orphaned/zombie PTY threads on exit or reload.

### ~~TD-006: No mouse event handling~~ — RESOLVED

### ~~TD-007: No clipboard integration~~ — RESOLVED

### ~~TD-010: Nerd Font icons render as CJK fallback glyphs~~ — RESOLVED

---

## Resolved Debt (Last 30 Days)

| ID | Title | Resolved | Resolution |
|----|-------|----------|------------|
| TD-039 | Robust ANSI Key Map | 2026-03-30 | Structured translate_key with xterm modifiers. |
| TD-034 | God Object Refactor | 2026-03-30 | App split into 4 specialized managers. |
| TD-037 | Palette AI Integration | 2026-03-30 | Command palette wired to AI logic. |
| TD-038 | AI UI Lua Configuration | 2026-03-30 | Panel appearance moved to Lua (llm.ui). |
| TD-032 | GPU Partial Uploads | 2026-03-30 | Dirty-row tracking for instance buffer. |
| TD-036 | Render Pass Consolidation | 2026-03-30 | BG + Glyph passes merged into one. |
| TD-005 | PTY JoinHandle | 2026-03-30 | std::thread JoinHandle + shutdown() loop. |
| TD-028 | Redundant Text Shaping | 2026-03-30 | Row-level caching (RowCache) with hashing. |
| TD-029 | O(N^2) Column Calculation | 2026-03-30 | Incremental column tracking in shape_line. |
| TD-033 | Atlas Stability & Eviction | 2026-03-30 | Flush-and-restart strategy on AtlasError::Full. |
| TD-025 | Vertical spacing too tight | 2026-03-27 | font.line_height config (default 1.2). |
| TD-018 | Powerline separator fringing | 2026-03-30 | Pixel snapping (floor) in vertex shader + manual blending in fragment shader. |
| TD-012 | Nerd Font icons overflow cell | 2026-03-23 | clamp_glyph_to_cell() crops glyph_size. |
| TD-041 | AI panel off-screen + broken upload | 2026-03-31 | resize_terminals_for_panel() on visibility change; full GPU upload when panel visible. |
| TD-040 | Leader Key Action Dispatch | 2026-04-03 | leader_map from config.keys; all custom binds via Lua; Action::FromStr. |
| TD-042 | Mouse selection + typing delay + font memory | 2026-04-03 | display_offset in selections; request_redraw on PTY data; remove per-frame font clone. |
