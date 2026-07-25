# GRAPH-ARCH-01 Remaining Domains Slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close out the remaining genuine duplicated-`Config`-logic cases found by a full sitewide field-by-field survey, completing the GRAPH-ARCH-01 effort.

**Architecture:** A survey of every field on the top-level `Config` struct (`src/config/schema.rs:6-27`) found the LLM domain (already fully migrated) was the only case with view-shaped duplication across 3+ files. Three smaller real duplications remain:

1. **`keys` + `leader.key`** — `src/app/input/mod.rs::InputHandler::new` and `src/ui/palette/actions.rs::built_in_actions` each independently re-filter `config.keys` for `kb.mods.eq_ignore_ascii_case("LEADER")`, then build two different derived maps from the filtered set. This is view-shaped (same predicate, 2 files, matches the `LlmRuntimeView` precedent) → gets a new `LeaderBindingsView` in `src/config/keybind_view.rs`.
2. **`font`** — `src/app/renderer/mod.rs`'s `RenderContext::new` and `RenderContext::refresh_text_metrics` both inline the identical 3-line sequence (clone `config.font`, scale by DPI, call `locate_font_for_lcd`). This is same-file, same-struct duplication → gets a private associated function, not a view.
3. **`max_fps`** — `src/app/frame.rs::flush_redraw_request` and `src/app/mod.rs`'s `ApplicationHandler::new_events` (around line 2081) both inline the identical `max_fps.max(1)` → nanosecond-interval formula. Same struct (`App`, split across `impl` blocks in two files) → gets a method, not a view.

Everything else on `Config` (`scrollback_lines`, `enable_scroll_bar`, `shell`, `shell_integration`, `input_ghost_text`, `input_syntax_highlight`, `keyboard`, `battery_saver`, `gpu_preference`, `notifications`, `workspaces`, plus the already-checked `window`/`colors`/`snippets`/`status_bar`) was surveyed and found to be 0-1 call sites or plain single-field pass-throughs — no view or helper needed, and no task in this plan touches them.

**Tech Stack:** Rust 2021, existing `src/config/llm_view.rs` as the established view-module pattern, `cargo test`.

## Global Constraints

- No config schema changes.
- No behavior changes anywhere. Every migrated/consolidated call site must produce identical output/behavior to what it replaced.
- Task 2 must NOT touch `RenderContext::scaled_font_config` (`src/app/renderer/mod.rs:278-282`, used by `src/app/frame.rs:501`) — that method is missing the `locate_font_for_lcd` step compared to the two duplicates this task consolidates, which may or may not be intentional. Fixing that inconsistency is a separate, behavior-changing decision outside this plan's scope; do not "fix" it as a drive-by.
- Task 1 may also correct the pre-existing alphabetical-ordering nit in `src/config/mod.rs`'s `pub mod` list (a `cargo fmt` violation flagged by a prior slice's final review, commit `d9f6a66`), since Task 1 already touches that exact list — see Task 1 Step 2.

---

### Task 1: Add `LeaderBindingsView` and migrate its two consumers

**Files:**
- Create: `src/config/keybind_view.rs`
- Modify: `src/config/mod.rs` (register the new module; also fix pre-existing mod-ordering)
- Modify: `src/app/input/mod.rs` (`InputHandler::new`, ~lines 76-85)
- Modify: `src/ui/palette/actions.rs` (`built_in_actions`, ~lines 117-126)

**Interfaces:**
- Consumes: `crate::config::schema::{Config, KeyBind}` (existing; `KeyBind { mods: String, key: String, action: String }`, derives `Clone`)
- Produces:
  - `pub struct LeaderBindingsView { pub leader_key: String, pub bindings: Vec<KeyBind> }`
  - `pub fn leader_bindings_view(config: &Config) -> LeaderBindingsView`

- [ ] **Step 1: Write the failing tests**

Create `src/config/keybind_view.rs`:

```rust
use super::schema::{Config, KeyBind};

#[derive(Debug, Clone)]
pub struct LeaderBindingsView {
    pub leader_key: String,
    pub bindings: Vec<KeyBind>,
}

pub fn leader_bindings_view(config: &Config) -> LeaderBindingsView {
    LeaderBindingsView {
        leader_key: config.leader.key.clone(),
        bindings: config
            .keys
            .iter()
            .filter(|kb| kb.mods.eq_ignore_ascii_case("LEADER"))
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kb(mods: &str, key: &str, action: &str) -> KeyBind {
        KeyBind {
            mods: mods.into(),
            key: key.into(),
            action: action.into(),
        }
    }

    #[test]
    fn leader_bindings_view_filters_to_leader_mods_only_case_insensitive() {
        let mut config = Config::default();
        config.keys = vec![
            kb("LEADER", "c", "NewTab"),
            kb("CMD", "k", "ClearScreen"),
            kb("leader", "x", "ClosePane"),
        ];
        let view = leader_bindings_view(&config);
        assert_eq!(view.bindings.len(), 2);
        assert!(view
            .bindings
            .iter()
            .any(|kb| kb.key == "c" && kb.action == "NewTab"));
        assert!(view
            .bindings
            .iter()
            .any(|kb| kb.key == "x" && kb.action == "ClosePane"));
    }

    #[test]
    fn leader_bindings_view_carries_leader_key() {
        let mut config = Config::default();
        config.leader.key = "f".into();
        let view = leader_bindings_view(&config);
        assert_eq!(view.leader_key, "f");
    }
}
```

- [ ] **Step 2: Register the module and fix the pre-existing ordering nit**

`src/config/mod.rs` currently starts:

```rust
pub mod lua;
pub mod llm_view;
pub mod schema;
pub mod watcher;
```

Replace with (adds `keybind_view` and fixes `llm_view`/`lua` alphabetical order — `llm_view` < `lua` alphabetically, so it belongs first; this was flagged as `cargo fmt` debt in a prior slice's final review):

```rust
pub mod keybind_view;
pub mod llm_view;
pub mod lua;
pub mod schema;
pub mod watcher;
```

- [ ] **Step 3: Run the new tests to verify they pass**

Run: `cargo test leader_bindings_view --lib`
Expected: PASS (2/2)

Run: `cargo fmt --check`
Expected: PASS (no ordering violations in `src/config/mod.rs` anymore)

- [ ] **Step 4: Migrate `InputHandler::new`**

Current code in `src/app/input/mod.rs`:

```rust
    pub fn new(config: &Config) -> Self {
        let leader_map = config
            .keys
            .iter()
            .filter(|kb| kb.mods.eq_ignore_ascii_case("LEADER"))
            .filter_map(|kb| {
                let action = kb.action.parse::<Action>().ok()?;
                Some((kb.key.clone(), action))
            })
            .collect();
```

Replace with:

```rust
    pub fn new(config: &Config) -> Self {
        let leader_view = crate::config::keybind_view::leader_bindings_view(config);
        let leader_map = leader_view
            .bindings
            .iter()
            .filter_map(|kb| {
                let action = kb.action.parse::<Action>().ok()?;
                Some((kb.key.clone(), action))
            })
            .collect();
```

Leave the rest of `new()` (the `Self { ... }` struct literal, `config.leader.timeout_ms` on the `leader_timeout_ms` field, etc.) untouched.

- [ ] **Step 5: Migrate `built_in_actions`**

Current code in `src/ui/palette/actions.rs`:

```rust
pub fn built_in_actions(config: &Config) -> Vec<PaletteAction> {
    // Build a lookup: Action string → formatted keybind label.
    let leader_label = format!("^{}", config.leader.key.to_uppercase());
    let mut keybind_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for kb in &config.keys {
        if kb.mods.eq_ignore_ascii_case("LEADER") {
            keybind_map.insert(kb.action.clone(), format!("{} {}", leader_label, kb.key));
        }
    }
```

Replace with:

```rust
pub fn built_in_actions(config: &Config) -> Vec<PaletteAction> {
    // Build a lookup: Action string → formatted keybind label.
    let leader_view = crate::config::keybind_view::leader_bindings_view(config);
    let leader_label = format!("^{}", leader_view.leader_key.to_uppercase());
    let mut keybind_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for kb in &leader_view.bindings {
        keybind_map.insert(kb.action.clone(), format!("{} {}", leader_label, kb.key));
    }
```

Note: the `if kb.mods.eq_ignore_ascii_case("LEADER")` check is correctly dropped — `leader_view.bindings` is already pre-filtered to exactly that predicate, so every `kb` in the loop already satisfies it.

- [ ] **Step 6: Build and run the full test suite**

Run: `cargo build`
Expected: PASS, no new warnings.

Run: `cargo test`
Expected: PASS, prior baseline (117 passed across 3 suites, per the previous slice's final review) plus this task's 2 new `leader_bindings_view` tests.

- [ ] **Step 7: Manual sanity check**

Run the app, open the command palette (`Leader o`) and confirm keybind labels next to actions (e.g. "New Tab" showing its leader keybind) still display correctly. Press a leader-bound key (e.g. `Leader c` for new tab) and confirm it still dispatches.

- [ ] **Step 8: Commit**

```bash
git add src/config/keybind_view.rs src/config/mod.rs src/app/input/mod.rs src/ui/palette/actions.rs
git commit -m "refactor: Add LeaderBindingsView and migrate keybind consumers."
```

---

### Task 2: Consolidate the duplicated scaled-font-with-LCD-fixup sequence

**Files:**
- Modify: `src/app/renderer/mod.rs` (`impl RenderContext`, the `new` function ~line 186-192 and `refresh_text_metrics` ~line 285-290)

**Interfaces:**
- Consumes: `crate::config::schema::{Config, FontConfig}` (existing), `crate::font::loader::locate_font_for_lcd` (existing)
- Produces: `RenderContext::locate_scaled_font(config: &Config, scale_factor: f32) -> FontConfig` (new private associated function)

- [ ] **Step 1: Confirm current code matches expectations**

Run: `sed -n '186,193p' src/app/renderer/mod.rs` — expect:

```rust
        let mut scaled_font = config.font.clone();
        scaled_font.size *= scale_factor;
        crate::font::loader::locate_font_for_lcd(&mut scaled_font);
```

Run: `sed -n '277,291p' src/app/renderer/mod.rs` — expect the existing `scaled_font_config` method (lines ~278-282, do NOT touch) immediately followed by `refresh_text_metrics` (starting ~284), whose body at ~288-290 has the identical 3 lines shown above.

If either doesn't match, stop and re-read the function before editing.

- [ ] **Step 2: Add the consolidated associated function**

Add this method inside `impl RenderContext` (place it directly above `scaled_font_config`, which stays completely unmodified):

```rust
    /// Font config scaled to physical pixels with LCD-safe font substitution applied.
    /// Used at startup and on DPI/font-reload — kept as one function so the two
    /// stay in lockstep instead of drifting independently.
    fn locate_scaled_font(config: &Config, scale_factor: f32) -> crate::config::schema::FontConfig {
        let mut cfg = config.font.clone();
        cfg.size *= scale_factor;
        crate::font::loader::locate_font_for_lcd(&mut cfg);
        cfg
    }
```

- [ ] **Step 3: Replace both call sites**

In `new()`, replace:

```rust
        let mut scaled_font = config.font.clone();
        scaled_font.size *= scale_factor;
        crate::font::loader::locate_font_for_lcd(&mut scaled_font);
```

with:

```rust
        let scaled_font = Self::locate_scaled_font(config, scale_factor);
```

In `refresh_text_metrics()`, replace the identical three lines with the identical replacement:

```rust
        let scaled_font = Self::locate_scaled_font(config, scale_factor);
```

Do not touch `scaled_font_config` (the existing `&self` method used by `src/app/frame.rs:501`) — it intentionally stays as-is per this plan's Global Constraints.

- [ ] **Step 4: Build and run the full test suite**

Run: `cargo build`
Expected: PASS, no new warnings, no unused-import warnings (both call sites still use `scaled_font` the same way afterward).

Run: `cargo test`
Expected: PASS, same counts as after Task 1.

- [ ] **Step 5: Manual sanity check**

Run the app, confirm fonts render correctly at startup, then trigger a DPI or font-config change (e.g. resize across a display boundary, or reload config after editing `font.size`) and confirm `refresh_text_metrics` still applies correctly (no font/glyph regressions).

- [ ] **Step 6: Commit**

```bash
git add src/app/renderer/mod.rs
git commit -m "refactor: Consolidate duplicated scaled-font-with-LCD-fixup sequence."
```

---

### Task 3: Consolidate the duplicated `max_fps` interval formula

**Files:**
- Modify: `src/app/frame.rs` (`impl App`, `flush_redraw_request`, ~lines 87-97)
- Modify: `src/app/mod.rs` (`impl ApplicationHandler<()> for App`, ~lines 2081-2084)

**Interfaces:**
- Consumes: `self.config.max_fps: u32` (existing field)
- Produces: `App::frame_interval(&self) -> std::time::Duration` (new `pub(super)` method, defined in `frame.rs`'s `impl App` block alongside `flush_redraw_request`, which is already `pub(super)` and already called cross-file from `mod.rs` — same visibility pattern)

- [ ] **Step 1: Confirm current code matches expectations**

Run: `sed -n '87,98p' src/app/frame.rs` — expect:

```rust
    pub(super) fn flush_redraw_request(&mut self) {
        if !self.needs_redraw {
            return;
        }
        // Enforce max_fps cap. If too soon since last frame, leave needs_redraw=true
        // so about_to_wait schedules a WaitUntil at the next frame deadline.
        let fps = self.config.max_fps.max(1) as u64;
        let interval = std::time::Duration::from_nanos(1_000_000_000 / fps);
        if self.last_frame_at.elapsed() < interval {
            return;
        }
```

Run: `sed -n '2079,2088p' src/app/mod.rs` — expect:

```rust
        // If flush_redraw_request deferred a frame due to max_fps, compute when
        // the next frame slot opens so we can wake at exactly that time.
        let frame_deadline: Option<std::time::Instant> = if self.needs_redraw {
            let fps = self.config.max_fps.max(1) as u64;
            let interval = std::time::Duration::from_nanos(1_000_000_000 / fps);
            Some(self.last_frame_at + interval)
        } else {
            None
        };
```

If either doesn't match, stop and re-read before editing.

- [ ] **Step 2: Add the method to `impl App` in `frame.rs`**

Add directly above `flush_redraw_request`:

```rust
    /// Minimum duration between frames, derived from `config.max_fps` (never zero,
    /// even if `max_fps` is misconfigured to 0).
    pub(super) fn frame_interval(&self) -> std::time::Duration {
        let fps = self.config.max_fps.max(1) as u64;
        std::time::Duration::from_nanos(1_000_000_000 / fps)
    }
```

- [ ] **Step 3: Replace both call sites**

In `src/app/frame.rs::flush_redraw_request`, replace:

```rust
        let fps = self.config.max_fps.max(1) as u64;
        let interval = std::time::Duration::from_nanos(1_000_000_000 / fps);
```

with:

```rust
        let interval = self.frame_interval();
```

In `src/app/mod.rs`, replace:

```rust
            let fps = self.config.max_fps.max(1) as u64;
            let interval = std::time::Duration::from_nanos(1_000_000_000 / fps);
```

with:

```rust
            let interval = self.frame_interval();
```

- [ ] **Step 4: Build and run the full test suite**

Run: `cargo build`
Expected: PASS, no new warnings.

Run: `cargo test`
Expected: PASS, same counts as after Task 2.

- [ ] **Step 5: Manual sanity check**

Run the app, confirm frame pacing still behaves the same (no visible change — this is a pure extraction of an identical formula).

- [ ] **Step 6: Commit**

```bash
git add src/app/frame.rs src/app/mod.rs
git commit -m "refactor: Consolidate duplicated max_fps interval formula into App::frame_interval."
```

## Self-Review

- **Spec coverage:** All three real duplication cases found by the sitewide survey are addressed — one view (matching the `LlmRuntimeView` shape, for the one case that actually had multi-file view-shaped duplication) and two small consolidation methods (for the two same-struct cases that didn't need the view machinery). Every other `Config` field was surveyed and correctly excluded with a stated reason.
- **Placeholder scan:** No TBD/TODO; every step has literal before/after code.
- **Type consistency:** `LeaderBindingsView { leader_key: String, bindings: Vec<KeyBind> }` and `leader_bindings_view(config: &Config) -> LeaderBindingsView` used identically in both consumer migrations in Task 1. `RenderContext::locate_scaled_font` and `App::frame_interval` signatures are each used identically at both of their call sites.
