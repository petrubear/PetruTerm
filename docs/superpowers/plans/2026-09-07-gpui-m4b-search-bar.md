# gpui M4b: Search Bar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `Cmd+F` opens an in-terminal text search bar in the gpui shell -- searches the focused
pane's grid (visible rows + full scrollback), highlights every match with the current one visually
distinct, Up/Down and Enter step through matches, Escape closes.

**Architecture:** Reuse `crate::ui::search_bar::SearchBar` directly (pure, engine-agnostic state
machine, zero wgpu coupling). The one real prerequisite: `Mux::search_active_terminal`/`Mux::
filter_matches` (the actual grid-scanning algorithm, rayon-parallelized) currently live on `Mux`,
which `gpui_shell` doesn't use -- Task 1 extracts both into free functions over `&Terminal` directly
(`src/term/search.rs`), with `Mux`'s own methods becoming one-line wrappers, preserving the wgpu
build's behavior and its existing test exactly. The UI (Task 2) is a non-modal `TextInput`-based
overlay (terminal stays interactive underneath, unlike M4a's modal palette) driving the same
dirty/scroll_needed algorithm the wgpu build's `frame.rs` already has, ported verbatim. Rendering the
highlights (Task 3) hooks into `rasterize_grid`'s existing per-cell color-resolution loop with a
pre-built per-line lookup index, mirroring the wgpu build's own `search_highlight_at` approach exactly.

**Tech Stack:** Rust, gpui 0.2.2, alacritty_terminal, rayon (existing dependency, already used by the
algorithm being extracted).

**Spec:** `docs/superpowers/specs/2026-09-07-gpui-m4-remaining-surfaces-design.md` (§4 M4b design, §8
M4b manual-test checklist, §9 Global Constraints).

## Global Constraints

- 400-line module limit. `input.rs` is exactly 400 lines and untouched by this plan (no new
  `input.rs` guards needed -- the search query's own focus guard lives in a new file, mirroring
  M4a's `palette.rs`). `pane_view.rs` (398) and `rasterize.rs` (814, already over from pre-existing
  work) are both touched by Task 3 in small, fixed-size ways -- watch their line counts after Task 3
  and flag if `pane_view.rs` crosses 400.
- `scripts/ci-local.sh` must stay green after every task (clippy `-D warnings` included).
- Commit format: `type: Message.` per `AGENTS.md`.
- Key/focus guards key on real focus (`is_focused(window)`), never on visibility/open state. The
  search query's `TextInput` is a real focus-grabbing widget, same treatment as M4a's palette query
  field -- no `InfoOverlay`-style visibility exception.
- The search bar itself is **non-modal**: no backdrop, no `cx.stop_propagation()` on background
  clicks. The terminal stays interactive underneath while it's open, unlike M4a's palette.
- Tests for logic only -- no painting/hit-testing tests (dogfooded). Task 1 is the one place this
  plan touches an *existing* test (`push_search_match_truncates_only_after_limit_is_exceeded`) -- it
  must keep passing with zero behavior change, not be rewritten.
- `#[allow(dead_code)]` (narrowly scoped, comment naming the removing task) for anything built ahead
  of its first real caller.

---

## Task 1: Extract the search algorithm to `crate::term::search`; wire `SearchBar` state + `Cmd+F`

**Purpose:** The one genuine wgpu-side refactor this plan needs, plus pure `gpui_shell` plumbing --
no UI yet. Model note: this task touches `src/app/mux/mod.rs` (outside `gpui_shell`, the first time
this migration has touched wgpu-side code) with real cross-cutting test-preservation concerns --
dispatch it on a standard-tier model, not the cheapest, unlike every other task in this plan.

**Files:**
- Create: `src/term/search.rs`
- Modify: `src/term/mod.rs` (register the module)
- Modify: `src/app/mux/mod.rs` (shrink `search_active_terminal`/`filter_matches` to wrappers; remove
  the moved constant/function/test)
- Modify: `src/gpui_shell/mod.rs`
- Modify: `src/gpui_shell/leader.rs`
- Modify: `src/gpui_shell/leader_dispatch.rs`

**Interfaces:**
- Consumes: `crate::term::Terminal` (`with_term`, already used throughout `gpui_shell`),
  `crate::ui::search_bar::{SearchBar, SearchMatch}`.
- Produces: `crate::term::search::{search_terminal(terminal: &Terminal, query: &str) -> (Vec<SearchMatch>, bool), filter_matches(terminal: &Terminal, prev: &[SearchMatch], new_query: &str) -> (Vec<SearchMatch>, bool), push_search_match(matches: &mut Vec<SearchMatch>, grid_line: i32, col: usize, len: usize) -> bool, MAX_SEARCH_MATCHES: usize}`; `GpuiShellRoot::{search_bar: SearchBar, search_query: Entity<TextInput>}`. `Cmd+F` is a standalone modifier combo, not a leader chord (same shape as the existing `Ctrl+Space` check), so this task adds no `LeaderAction` variant -- it adds a new top-of-`on_key_down` standalone check directly in this task (unlike M4a, where the keybind waited for Task 2's render to exist: `Cmd+F` needs no companion render to be worth wiring now, and toggling `search_bar.visible` with no visible effect until Task 2's render exists is the same "no UI yet" shape M4a's own Task 1 had).

- [ ] **Step 1: Write `src/term/search.rs`**

```rust
// gpui chrome migration (M4b Task 1): the terminal grid text-search
// algorithm -- extracted from `src/app/mux/mod.rs`'s `Mux::
// search_active_terminal`/`Mux::filter_matches`, which only ever needed
// `&Terminal` internally (via `Mux::active_terminal()` then `terminal.
// with_term(..)` for everything else). `Mux`'s own methods become
// one-line wrappers below this file's functions -- their behavior and
// the wgpu build's own `push_search_match_truncates_only_after_limit_
// is_exceeded` test (moved here, unchanged) are both preserved exactly.
// `gpui_shell` calls these functions directly on whichever `Rc<Terminal>`
// the focused pane holds, the same "used directly, never copied"
// relationship this migration has established for every other
// engine-agnostic piece of wgpu-build logic it reuses.

use alacritty_terminal::index::{Column, Line};

use crate::term::Terminal;
use crate::ui::search_bar::SearchMatch;

pub(crate) const MAX_SEARCH_MATCHES: usize = 10_000;

pub(crate) fn push_search_match(
    matches: &mut Vec<SearchMatch>,
    grid_line: i32,
    col: usize,
    len: usize,
) -> bool {
    if matches.len() >= MAX_SEARCH_MATCHES {
        return true;
    }
    matches.push(SearchMatch {
        grid_line,
        col,
        len,
    });
    false
}

/// Search all visible rows and scrollback history for `query` (case-insensitive).
/// Returns matches sorted from oldest history to current screen.
///
/// Implementation uses a collect-then-parallel strategy:
///   Phase 1 (serial, lock held): read the terminal grid into a flat Vec<char>.
///   Phase 2 (parallel, lock released): scan the flat buffer with rayon par_chunks.
/// This keeps the term lock held for only ~O(rows*cols) char copies instead of the
/// full search duration, and parallelizes the CPU-bound scan across all cores.
/// For short scrollback (< PAR_THRESHOLD rows) the serial path is used instead
/// because rayon fork-join overhead (~50 µs) exceeds the serial scan time.
pub fn search_terminal(terminal: &Terminal, query: &str) -> (Vec<SearchMatch>, bool) {
    use rayon::prelude::*;
    const PAR_THRESHOLD: usize = 400;

    if query.is_empty() {
        return (Vec::new(), false);
    }
    let query_lower = query.to_lowercase();
    let query_chars: Vec<char> = query_lower.chars().collect();
    let query_len = query_chars.len();

    // Phase 1: collect the entire grid into a flat char buffer while holding the lock.
    // A single flat allocation (rows * cols chars) avoids per-row Vec overhead and
    // produces a contiguous layout suitable for par_chunks in phase 2.
    let (history_i32, cols, flat) = terminal.with_term(|term| {
        let screen_rows = term.screen_lines() as i32;
        let history = term.grid().history_size() as i32;
        let cols = term.columns();
        let total = (history + screen_rows) as usize;
        let mut flat = Vec::with_capacity(total * cols);
        for grid_row in (-history)..screen_rows {
            let line = Line(grid_row);
            for col in 0..cols {
                let c = term.grid()[line][Column(col)].c;
                let c = if c == '\0' { ' ' } else { c };
                flat.push(c.to_lowercase().next().unwrap_or(c));
            }
        }
        (history, cols, flat)
    }); // Term lock released here.

    if cols == 0 || query_chars.is_empty() {
        return (Vec::new(), false);
    }

    let total_rows = flat.len() / cols;

    if total_rows < PAR_THRESHOLD {
        // Serial path: short scrollback or small terminal.
        let mut matches = Vec::new();
        for (chunk_idx, row_chars) in flat.chunks(cols).enumerate() {
            let grid_row = chunk_idx as i32 - history_i32;
            let scan_end = row_chars.len().saturating_sub(query_len.saturating_sub(1));
            for col in 0..scan_end {
                if row_chars[col..col + query_len] == query_chars[..]
                    && push_search_match(&mut matches, grid_row, col, query_len)
                {
                    return (matches, true);
                }
            }
        }
        (matches, false)
    } else {
        // Parallel path: rayon par_chunks — each task scans one row.
        // collect() preserves row order (rayon guarantees output order matches input).
        // No early exit: collect all matches then truncate to MAX_SEARCH_MATCHES.
        // `qc` is a &[char] slice (Copy) so it can be moved into per-row closures
        // without consuming `query_chars`.
        let qc: &[char] = &query_chars;
        let ql = query_len;
        let hi = history_i32;
        let mut matches: Vec<SearchMatch> = flat
            .par_chunks(cols)
            .enumerate()
            .flat_map_iter(|(chunk_idx, row_chars)| {
                let grid_row = chunk_idx as i32 - hi;
                let scan_end = row_chars.len().saturating_sub(ql.saturating_sub(1));
                (0..scan_end).filter_map(move |col| {
                    if row_chars[col..col + ql] == qc[..] {
                        Some(SearchMatch {
                            grid_line: grid_row,
                            col,
                            len: ql,
                        })
                    } else {
                        None
                    }
                })
            })
            .collect();

        let truncated = matches.len() > MAX_SEARCH_MATCHES;
        matches.truncate(MAX_SEARCH_MATCHES);
        (matches, truncated)
    }
}

/// Incremental filter: given matches from a previous (shorter) query, verify each one
/// against the new (longer) query. O(prev_matches × query_len) instead of O(rows × cols).
/// Only valid when `new_query.starts_with(prev_query)` — caller is responsible for this check.
pub fn filter_matches(
    terminal: &Terminal,
    prev: &[SearchMatch],
    new_query: &str,
) -> (Vec<SearchMatch>, bool) {
    if new_query.is_empty() || prev.is_empty() {
        return (Vec::new(), false);
    }
    let q_lower = new_query.to_lowercase();
    let q_chars: Vec<char> = q_lower.chars().collect();
    let q_len = q_chars.len();
    terminal.with_term(|term| {
        let cols = term.columns();
        let mut matches = Vec::with_capacity(prev.len().min(MAX_SEARCH_MATCHES));
        for m in prev {
            if m.col + q_len > cols {
                continue;
            }
            let line = Line(m.grid_line);
            let mut matched = true;
            for (i, &qc) in q_chars.iter().enumerate() {
                let c = term.grid()[line][Column(m.col + i)].c;
                let c = if c == '\0' { ' ' } else { c };
                if c.to_lowercase().next().unwrap_or(c) != qc {
                    matched = false;
                    break;
                }
            }
            if matched && push_search_match(&mut matches, m.grid_line, m.col, q_len) {
                return (matches, true);
            }
        }
        (matches, false)
    })
}

#[cfg(test)]
mod tests {
    use super::{push_search_match, MAX_SEARCH_MATCHES};
    use crate::ui::search_bar::SearchMatch;

    #[test]
    fn push_search_match_truncates_only_after_limit_is_exceeded() {
        let mut matches: Vec<SearchMatch> = Vec::new();
        for col in 0..MAX_SEARCH_MATCHES {
            assert!(!push_search_match(&mut matches, 0, col, 1));
        }
        assert_eq!(matches.len(), MAX_SEARCH_MATCHES);
        assert!(push_search_match(&mut matches, 0, MAX_SEARCH_MATCHES, 1));
        assert_eq!(matches.len(), MAX_SEARCH_MATCHES);
    }
}
```

- [ ] **Step 2: Register the module**

In `src/term/mod.rs`, add `pub mod search;` to the module list, alphabetically after `pub mod pty;`
and before `pub mod tokenizer;` (matches the existing `pub mod` style already used for `blocks`,
`color`, `flag_db`, `input_shadow`, `osc133`, `pty` in that file).

- [ ] **Step 3: Shrink `Mux::search_active_terminal`/`Mux::filter_matches` to wrappers**

In `src/app/mux/mod.rs`, replace the full body of `search_active_terminal` (everything from `pub fn
search_active_terminal(&self, query: &str) -> (Vec<SearchMatch>, bool) {` through its matching
closing `}`) with:

```rust
    /// Search all visible rows and scrollback history for `query` (case-insensitive).
    /// Returns matches sorted from oldest history to current screen. The real algorithm
    /// lives in `crate::term::search::search_terminal` (M4b) -- this is a thin wrapper
    /// resolving the active terminal, kept for API compatibility with existing callers.
    pub fn search_active_terminal(&self, query: &str) -> (Vec<SearchMatch>, bool) {
        match self.active_terminal() {
            Some(terminal) => crate::term::search::search_terminal(terminal, query),
            None => (Vec::new(), false),
        }
    }
```

Replace the full body of `filter_matches` (everything from `pub fn filter_matches(&self, prev: &[SearchMatch], new_query: &str) -> (Vec<SearchMatch>, bool) {` through its matching closing `}`) with:

```rust
    /// Incremental filter: given matches from a previous (shorter) query, verify each one
    /// against the new (longer) query (TD-PERF-11). The real algorithm lives in
    /// `crate::term::search::filter_matches` (M4b) -- this is a thin wrapper.
    pub fn filter_matches(
        &self,
        prev: &[SearchMatch],
        new_query: &str,
    ) -> (Vec<SearchMatch>, bool) {
        match self.active_terminal() {
            Some(terminal) => crate::term::search::filter_matches(terminal, prev, new_query),
            None => (Vec::new(), false),
        }
    }
```

Delete `const MAX_SEARCH_MATCHES: usize = 10_000;` and the whole `fn push_search_match(...) -> bool { ... }` function (both now live in `src/term/search.rs`, Step 1) from `src/app/mux/mod.rs`.

Delete the whole `#[test] fn push_search_match_truncates_only_after_limit_is_exceeded() { ... }` test
from `src/app/mux/mod.rs`'s own `#[cfg(test)] mod tests` block (moved to `src/term/search.rs`, Step
1). Update that test module's import line from:

```rust
    use super::{drain_pty_events, push_search_match, MAX_SEARCH_MATCHES, PTY_EVENT_WORK_BUDGET};
```

to:

```rust
    use super::{drain_pty_events, PTY_EVENT_WORK_BUDGET};
```

- [ ] **Step 4: `gpui_shell/mod.rs` -- `SearchBar` field**

Add to the imports (alongside the existing `use crate::ui::palette::{Action, CommandPalette};`):

```rust
use crate::ui::search_bar::SearchBar;
```

Add to the `GpuiShellRoot` struct, right after the `pending_palette_action` field:

```rust
    pending_palette_action: Option<Action>,
    /// In-terminal text search (`Cmd+F`) -- `crate::ui::search_bar::
    /// SearchBar`, used directly, same relationship as `CommandPalette`.
    /// Unlike the palette, this drives real GPU-paint highlighting (M4b
    /// Task 3) rather than only its own popup content.
    search_bar: SearchBar,
    /// The search query's own persistent `TextInput` entity -- same
    /// "cleared and refocused on each open, not rebuilt" shape as the
    /// palette's `palette_query` (M4a).
    search_query: gpui::Entity<text_input::TextInput>,
```

Add to `new()`, right after the `cx.subscribe(&palette_query, ...)` block's `.detach();` line:

```rust
        let search_bar = SearchBar::default();
        let search_query =
            cx.new(|cx| text_input::TextInput::new(cx, &config.colors, "", "Search..."));
        cx.subscribe(&search_query, |this, _input, event, cx| {
            match event {
                text_input::TextInputEvent::Submit => this.search_bar.next_match(),
                text_input::TextInputEvent::Cancel => this.search_bar.close(),
            }
            cx.notify();
        })
        .detach();
```

Add to the `Self { .. }` literal, right after `pending_palette_action: None,`:

```rust
            pending_palette_action: None,
            search_bar,
            search_query,
```

- [ ] **Step 5: `Cmd+F` keybind in `input.rs`**

Add this as a new standalone check in `on_key_down`, right after the existing `Ctrl+Space` block
(the `AI block toggle` check that ends with its own `return;`), before the `Cmd+1-9` check:

```rust
        // ── Cmd+F — toggle the in-terminal search bar ────────────────────
        // Standalone combo, not a leader chord, same shape as `Ctrl+Space`
        // above. Only reachable once neither composer/palette/overlay guard
        // above already returned, so it can't be swallowed mid-edit.
        if event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.shift
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && event.keystroke.key == "f"
        {
            if self.search_bar.visible {
                self.search_bar.close();
                window.focus(&self.focus_handle);
            } else {
                self.search_query
                    .update(cx, |input, cx| input.set_content("", cx));
                self.search_bar.open();
                self.search_query.focus_handle(cx).focus(window);
            }
            cx.notify();
            return;
        }

```

- [ ] **Step 6: Build, test, verify**

Run: `cargo build 2>&1 | tail -80` -- zero errors, zero warnings.

Run: `cargo test --lib 2>&1 | tail -15` -- 230/230 passing, including
`term::search::tests::push_search_match_truncates_only_after_limit_is_exceeded` (moved, same
assertions) and confirming `app::mux::tests::push_search_match_truncates_only_after_limit_is_exceeded`
no longer exists as a separate test (it moved, not duplicated).

Run: `./scripts/ci-local.sh` -- must exit 0.

Dogfood: launch the app. `Cmd+F` should toggle `self.search_bar.visible` with no visible effect (no
render yet) -- confirms this task's plumbing doesn't break startup, existing keybinds, or the wgpu
build's own `cargo build`/`cargo test` (run `cargo test --lib mux::` too, to be sure the wrapper
methods still pass whatever tests reference them indirectly through other `Mux` tests, even though
`search_active_terminal`/`filter_matches` have no direct tests of their own beyond the moved one).

- [ ] **Step 7: Commit**

```bash
git add src/term/search.rs src/term/mod.rs src/app/mux/mod.rs src/gpui_shell/mod.rs src/gpui_shell/input.rs
git commit -m "feat: Extract the terminal search algorithm and wire SearchBar state + Cmd+F (M4b Task 1)."
```

---

## Task 2: Render the search bar; port the dirty/scroll_needed driver logic

**Purpose:** The first task with anything visible: `Cmd+F` shows a real, working (non-highlighting
yet -- Task 3's job) search bar. Typing runs real searches against the focused terminal's grid via
Task 1's extracted algorithm; the match counter is accurate; Up/Down/Enter navigate; the terminal
scrolls to bring the current match into view.

**Files:**
- Create: `src/gpui_shell/search_bar.rs`
- Modify: `src/gpui_shell/mod.rs` (register module)
- Modify: `src/gpui_shell/input.rs` (the query field's own key guard)
- Modify: `src/gpui_shell/render.rs` (the driver logic + wire the bar into the tree)

**Interfaces:**
- Consumes: `SearchBar::{query, matches, current, visible, dirty, scroll_needed, last_query, matches_truncated, set_matches, next_match, prev_match, current_match, count_label}` (all pre-existing), `crate::term::search::{search_terminal, filter_matches}` (Task 1), `Terminal::{scrollback_info, rows, scroll_display}` (pre-existing).
- Produces: `search_bar::render_search_bar(search: &SearchBar, query_input: &Entity<TextInput>, colors: &ColorScheme) -> impl IntoElement`; `GpuiShellRoot::{search_query_focused(&self, window, cx) -> bool, maybe_handle_search_key(&mut self, event, window, cx) -> bool}`.

- [ ] **Step 1: Write `src/gpui_shell/search_bar.rs`**

```rust
// gpui chrome migration (M4b Task 2): the search bar's render tree and its
// own keyboard guard. State (`SearchBar`) is reused directly from
// `crate::ui::search_bar` -- see `mod.rs`'s own doc comment on the
// `search_bar` field for why. Deliberately non-modal (no backdrop, no
// `cx.stop_propagation()`): the terminal stays interactive underneath
// while this is open, unlike M4a's command palette.

use gpui::{div, prelude::*, px, App, Context, Entity, KeyDownEvent, Window};

use crate::config::schema::ColorScheme;
use crate::ui::search_bar::SearchBar;

use super::font_state;
use super::pane_view::to_rgba;
use super::text_input::TextInput;
use super::GpuiShellRoot;

/// Build the search bar's `div()` tree: a small row anchored to the top of
/// the pane area, holding the query field and a match-count label.
pub fn render_search_bar(
    search: &SearchBar,
    query_input: &Entity<TextInput>,
    colors: &ColorScheme,
) -> impl IntoElement {
    div()
        .id("search-bar")
        .absolute()
        .top_0()
        .right_0()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .bg(to_rgba(colors.ui_surface))
        .border_1()
        .border_color(to_rgba(colors.ui_border))
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()))
        .child(div().w(px(200.0)).child(query_input.clone()))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(to_rgba(colors.ui_muted))
                .child(search.count_label()),
        )
}

impl GpuiShellRoot {
    /// True while the search query field genuinely holds keyboard focus --
    /// same shape as `palette_query_focused` (M4a's `palette.rs`).
    pub(super) fn search_query_focused(&self, window: &Window, cx: &App) -> bool {
        self.search_query.focus_handle(cx).is_focused(window)
    }

    /// The search bar's own key guard, called from `input.rs`'s
    /// `on_key_down`. Returns `true` if the key was consumed. Up/Down move
    /// to the prev/next match directly (`SearchBar::prev_match`/
    /// `next_match`, TextInput has no binding for either -- verified in
    /// M4a). Enter/Escape are TextInput's own bound `Submit`/`Cancel`
    /// actions, handled via `search_query`'s `cx.subscribe` callback in
    /// `mod.rs` instead, same mechanism M4a's command palette established.
    pub(super) fn maybe_handle_search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.search_query_focused(window, cx) {
            return false;
        }
        match event.keystroke.key.as_str() {
            "down" => self.search_bar.next_match(),
            "up" => self.search_bar.prev_match(),
            _ => {}
        }
        cx.notify();
        true
    }
}
```

- [ ] **Step 2: Register the module**

In `src/gpui_shell/mod.rs`, add `mod search_bar;` to the module list, alphabetically after `mod
rename;` and before `mod render;`. This does not collide with Task 1's own `use crate::ui::search_bar
::SearchBar;` import: that statement brings only the `SearchBar` type into scope, not a binding named
`search_bar`, so a local `mod search_bar` declaring `gpui_shell::search_bar` (this file's render
module) coexists with it cleanly.

- [ ] **Step 3: `input.rs` -- the search query's key guard**

Add this guard right after the palette's own guard (`if self.maybe_handle_palette_key(event, window, cx) { return; }`), before the `InfoOverlay` guard:

```rust
        // The search bar's own key guard -- see `search_bar.rs`'s own doc
        // comment on `maybe_handle_search_key` for the full reasoning.
        if self.maybe_handle_search_key(event, window, cx) {
            return;
        }

```

- [ ] **Step 4: `render.rs` -- the driver logic + wiring into the tree**

Add `use super::search_bar;` to the imports, alongside the existing `use super::palette;`.

Add this block as the very first statement in `render()`, right after the palette's own
`pending_palette_action` drain block (before the `// Skipped while a child owns focus...` comment):

```rust
        // Search: run the query if dirty, scroll to the current match if
        // needed -- ported verbatim from the wgpu build's own driver
        // (`src/app/frame.rs`, lines 647-685), just moved from "once per
        // poll tick" to "once per render() call" (this file has no
        // separate per-tick hook the way `frame.rs` does, and `render()`
        // already runs every frame the poll loop wakes for).
        if self.search_bar.visible {
            let active_ws = self.workspaces.active();
            let active_tid = active_ws.tab_panes[active_ws.tabs.active_index()].focused_terminal;
            if let Some(terminal) = self.terminals.get(&active_tid).cloned() {
                if self.search_bar.dirty {
                    let query = self.search_bar.query.clone();
                    if query.is_empty() {
                        self.search_bar.set_matches(Vec::new(), false);
                    } else {
                        let prev_query = self.search_bar.last_query.clone();
                        let can_filter = !self.search_bar.matches.is_empty()
                            && !self.search_bar.matches_truncated
                            && query.starts_with(prev_query.as_str())
                            && !prev_query.is_empty();
                        let (matches, truncated) = if can_filter {
                            crate::term::search::filter_matches(
                                &terminal,
                                &self.search_bar.matches,
                                &query,
                            )
                        } else {
                            crate::term::search::search_terminal(&terminal, &query)
                        };
                        self.search_bar.set_matches(matches, truncated);
                    }
                    self.search_bar.last_query = query;
                    self.search_bar.dirty = false;
                }
                if self.search_bar.scroll_needed {
                    if let Some(m) = self.search_bar.current_match().cloned() {
                        let (disp_off, _) = terminal.scrollback_info();
                        let screen_rows = terminal.rows.get() as i32;
                        let target_offset = (screen_rows / 2 - m.grid_line).max(0) as usize;
                        let delta = disp_off as i32 - target_offset as i32;
                        if delta != 0 {
                            terminal.scroll_display(-delta);
                        }
                    }
                    self.search_bar.scroll_needed = false;
                }
            }
        }

```

Add the search bar to the `pane_area` div (which already has `.relative()`, needed for this overlay's
own `.absolute()` positioning) -- replace:

```rust
        let pane_area = div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(panes)
            .when(self.ai_block.is_visible(), |el| {
                el.child(ai_block::render_ai_block(
                    &self.ai_block,
                    &self.config.colors,
                ))
            });
```

with:

```rust
        let pane_area = div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(panes)
            .when(self.ai_block.is_visible(), |el| {
                el.child(ai_block::render_ai_block(
                    &self.ai_block,
                    &self.config.colors,
                ))
            })
            .when(self.search_bar.visible, |el| {
                el.child(search_bar::render_search_bar(
                    &self.search_bar,
                    &self.search_query,
                    &self.config.colors,
                ))
            });
```

- [ ] **Step 5: Build, test, verify**

Run: `cargo build 2>&1 | tail -80`, `cargo test --lib 2>&1 | tail -10` (230/230, no new tests -- no
new pure-logic, everything here is UI/wiring/algorithm-invocation exercised by dogfood),
`./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/input.rs src/gpui_shell/render.rs src/gpui_shell/search_bar.rs` and confirm
all three stay under 400 -- if `input.rs` or `render.rs` crossed it, note the exact line count in
your report; do not attempt a further split yourself, that's a controller decision.

Dogfood: `Cmd+F` opens a small bar top-right of the pane area, focuses the query field immediately
(type right away to confirm). Type a string that exists somewhere in your scrollback -- the count
label updates to show a real match count (not highlighted yet -- Task 3's job). Up/Down move which
match is "current" (confirm via the count label's numerator changing) and the terminal visibly
scrolls to bring each one into view. `Cmd+F` again closes it and returns focus to the terminal
(confirm by typing immediately after). Reopen, Escape closes the same way. Type a query with zero
matches -- count label reads "no results", nothing crashes.

- [ ] **Step 6: Commit**

```bash
git add src/gpui_shell/search_bar.rs src/gpui_shell/mod.rs src/gpui_shell/input.rs src/gpui_shell/render.rs
git commit -m "feat: Render the search bar and port the dirty/scroll driver logic (M4b Task 2)."
```

---

## Task 3: Highlight matches in the terminal grid

**Purpose:** The one genuinely new rendering work in M4b: paint every match with a highlight
background, the current match visually distinct from the rest, using the same pre-built per-line
index approach the wgpu build's own renderer already uses for O(1) per-cell lookup.

**Files:**
- Modify: `src/gpui_shell/rasterize.rs`
- Modify: `src/gpui_shell/terminal_element.rs`
- Modify: `src/gpui_shell/pane_view.rs`
- Modify: `src/gpui_shell/render.rs`

**Interfaces:**
- Consumes: `crate::ui::search_bar::SearchMatch` (`grid_line: i32, col: usize, len: usize`), the existing `resolve_cell_colors`/per-cell loop in `rasterize_grid` (unchanged in shape, extended).
- Produces: `rasterize::rasterize_grid`'s extended signature (adds a `search` parameter); `TerminalGridElement::search: Option<(Vec<SearchMatch>, usize)>`; `pane_view::PaneRenderCx::search: Option<(Vec<SearchMatch>, usize)>`.

- [ ] **Step 1: `rasterize.rs` -- the highlight index + color override**

Add to the imports, alongside the existing `use alacritty_terminal::selection::SelectionRange;`:

```rust
use crate::ui::search_bar::SearchMatch;
```

Add these three constants right after the existing `type CellColorStyle = ...` line (same file,
before `resolve_cell_colors`):

```rust
/// Highlight colors for search matches -- Dracula bg/yellow/orange, ported
/// verbatim from the wgpu build's own `SEARCH_MATCH_FG`/`SEARCH_MATCH_BG`/
/// `SEARCH_CURRENT_BG` (`src/app/mux/mod.rs`), converted from that file's
/// 0-255 `AnsiColor::Spec(Rgb {..})` literals to this file's own `[f32; 4]`
/// (0.0-1.0) color space -- same values, same visual result.
const SEARCH_MATCH_FG: [f32; 4] = [40.0 / 255.0, 42.0 / 255.0, 54.0 / 255.0, 1.0];
const SEARCH_MATCH_BG: [f32; 4] = [241.0 / 255.0, 250.0 / 255.0, 140.0 / 255.0, 1.0];
const SEARCH_CURRENT_BG: [f32; 4] = [255.0 / 255.0, 184.0 / 255.0, 108.0 / 255.0, 1.0];

/// Return overridden (fg, bg) colors if (grid_line, col) falls inside any
/// search match -- ported from the wgpu build's own `search_highlight_at`
/// (`src/app/mux/mod.rs`), same pre-built per-line index for O(1) line
/// lookup + O(matches_on_line) range check (TD-PERF-22), just returning
/// this file's `[f32; 4]` colors instead of `AnsiColor`.
fn search_highlight_at(
    grid_line: i32,
    col: usize,
    idx: &rustc_hash::FxHashMap<i32, Vec<(usize, usize, bool)>>,
) -> Option<([f32; 4], [f32; 4])> {
    for &(start, end, is_current) in idx.get(&grid_line)? {
        if col >= start && col < end {
            let bg = if is_current {
                SEARCH_CURRENT_BG
            } else {
                SEARCH_MATCH_BG
            };
            return Some((SEARCH_MATCH_FG, bg));
        }
    }
    None
}
```

Change `rasterize_grid`'s signature -- replace:

```rust
pub fn rasterize_grid(
    terminal: &Rc<Terminal>,
    cell_width: Pixels,
    cell_height: Pixels,
    scale: f32,
    colors: &ColorScheme,
    window: &mut Window,
) -> Option<Arc<RenderImage>> {
```

with:

```rust
pub fn rasterize_grid(
    terminal: &Rc<Terminal>,
    cell_width: Pixels,
    cell_height: Pixels,
    scale: f32,
    colors: &ColorScheme,
    window: &mut Window,
    search: Option<(&[SearchMatch], usize)>,
) -> Option<Arc<RenderImage>> {
```

Inside `rasterize_grid`, right after the existing `let sel_range: Option<SelectionRange> = ...;`
line, add:

```rust
        // Build a line-indexed search map once — O(matches) — so the
        // per-cell lookup below is O(1) (TD-PERF-22, ported from the wgpu
        // build's own `collect_grid_cells`). Keyed on buffer-space grid
        // line (matching `SearchMatch::grid_line`'s own semantics
        // directly, the same space `cell.point.line.0` below is in before
        // any viewport conversion) — no coordinate translation needed.
        let search_idx: rustc_hash::FxHashMap<i32, Vec<(usize, usize, bool)>> =
            if let Some((matches, current_idx)) = search {
                let mut idx: rustc_hash::FxHashMap<i32, Vec<(usize, usize, bool)>> =
                    rustc_hash::FxHashMap::default();
                for (i, m) in matches.iter().enumerate() {
                    idx.entry(m.grid_line)
                        .or_default()
                        .push((m.col, m.col + m.len, i == current_idx));
                }
                idx
            } else {
                rustc_hash::FxHashMap::default()
            };
```

Change the per-cell color resolution -- replace:

```rust
            let (fg, bg) = resolve_cell_colors(cell.fg, cell.bg, cell.flags, in_selection, colors);
            let style = CellStyle {
```

with:

```rust
            let (fg, bg) = resolve_cell_colors(cell.fg, cell.bg, cell.flags, in_selection, colors);
            // Search highlight overrides selection, not the reverse --
            // matches the wgpu build's own priority order in
            // `collect_grid_cells`.
            let (fg, bg) =
                search_highlight_at(cell.point.line.0, col, &search_idx).unwrap_or((fg, bg));
            let style = CellStyle {
```

- [ ] **Step 2: `terminal_element.rs` -- the new field + call-site update**

Add `use crate::ui::search_bar::SearchMatch;` to the imports.

Add a new field to `TerminalGridElement`, right after `pub is_active: bool,`:

```rust
    pub is_active: bool,
    /// Active matches for the currently-focused pane's search, plus which
    /// index is "current" -- `None` for every pane except the focused one
    /// (search always targets the focused terminal only, matching the
    /// wgpu build's own `Mux::focused_terminal_id()` scoping). Threaded
    /// through to `rasterize::rasterize_grid`'s own `search` parameter.
    pub search: Option<(Vec<SearchMatch>, usize)>,
```

Update the `rasterize::rasterize_grid(...)` call site -- replace:

```rust
        if let Some(render_image) = rasterize::rasterize_grid(
            &self.terminal,
            self.cell_width,
            self.cell_height,
            window.scale_factor(),
            &self.colors,
            window,
        ) {
```

with:

```rust
        let search_ref = self
            .search
            .as_ref()
            .map(|(matches, current)| (matches.as_slice(), *current));
        if let Some(render_image) = rasterize::rasterize_grid(
            &self.terminal,
            self.cell_width,
            self.cell_height,
            window.scale_factor(),
            &self.colors,
            window,
            search_ref,
        ) {
```

- [ ] **Step 3: `pane_view.rs` -- thread `search` through `PaneRenderCx`**

Add `use crate::ui::search_bar::SearchMatch;` to the imports.

Add a new field to `PaneRenderCx`, right after `pub on_drag: SeparatorDragCallback,`:

```rust
    pub on_drag: SeparatorDragCallback,
    /// Active search matches for the focused pane, threaded to whichever
    /// leaf's `terminal_id == focused` (M4b Task 3) -- every other leaf
    /// gets `None`.
    pub search: Option<(Vec<SearchMatch>, usize)>,
```

In `render_leaf`, update the `TerminalGridElement { .. }` literal -- replace:

```rust
        .child(TerminalGridElement {
            terminal,
            cell_width,
            cell_height,
            colors: ctx.colors.clone(),
            is_active: terminal_id == ctx.focused,
            cursor_blink_on: ctx.cursor_blink_on,
            on_focus,
        })
```

with:

```rust
        .child(TerminalGridElement {
            terminal,
            cell_width,
            cell_height,
            colors: ctx.colors.clone(),
            is_active: terminal_id == ctx.focused,
            cursor_blink_on: ctx.cursor_blink_on,
            on_focus,
            search: if terminal_id == ctx.focused {
                ctx.search.clone()
            } else {
                None
            },
        })
```

- [ ] **Step 4: `render.rs` -- populate `PaneRenderCx.search`**

Find the existing `let pane_ctx = pane_view::PaneRenderCx { .. };` construction and add one field to
its literal, right after `on_drag,`:

```rust
        let pane_ctx = pane_view::PaneRenderCx {
            terminals: &self.terminals,
            focused: self.workspaces.active().tab_panes[active_index].focused_terminal,
            colors: &self.config.colors,
            cell_width,
            cell_height,
            cursor_blink_on: self.cursor_blink_on,
            scrollback: self.config.scrollback_lines as usize,
            rects: self.rect_cache.clone(),
            on_focus,
            on_drag,
            search: self
                .search_bar
                .visible
                .then(|| (self.search_bar.matches.clone(), self.search_bar.current)),
        };
```

- [ ] **Step 5: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (230/230, unchanged -- no
pure-logic tests for painting, per this project's convention), `cargo clippy --all-features -- -D
warnings` (clean), `./scripts/ci-local.sh` (exit 0).

Run `wc -l src/gpui_shell/rasterize.rs src/gpui_shell/terminal_element.rs src/gpui_shell/pane_view.rs`
and note the results in your report -- `rasterize.rs` (814 before this task) and `terminal_element.rs`
(262 before this task) both already exceed or approach the 400-line convention from pre-existing work
outside this milestone's scope; this task's own addition to each should be small (roughly 20-30 lines
to `rasterize.rs`, under 15 to each of the other two) and is not expected to require a split, but flag
the exact resulting counts for the controller to judge rather than deciding yourself.

Dogfood, reproducing the M4 spec's §8 M4b checklist:
1. `Cmd+F`, type a query with real matches in the visible viewport -- every match highlights with a
   yellow background (Dracula yellow), the current one distinct (orange).
2. Scroll back to a match that only exists in scrollback history (type a query matching something you
   scrolled past) -- confirm it highlights too, not just on-screen matches.
3. Up/Down step through matches -- the highlighted "current" match moves, and the terminal scrolls to
   keep it roughly centered.
4. Match count label stays accurate throughout.
5. Close the search bar (`Cmd+F` or Escape) -- highlights disappear immediately (confirm: `SearchBar::
   close()` already clears `matches`, so the very next paint should show none).
6. With the search bar open, click into the terminal and type normally -- confirm it's NOT
   intercepted (the search bar is deliberately non-modal; this is the same class of check M4a's
   palette needed for its own, differently-shaped guard).
7. On a terminal with a long scrollback, type a common query (many matches) -- confirm no visible
   input stall (the rayon-parallel path is exactly what this checks).

- [ ] **Step 6: Commit**

```bash
git add src/gpui_shell/rasterize.rs src/gpui_shell/terminal_element.rs src/gpui_shell/pane_view.rs src/gpui_shell/render.rs
git commit -m "feat: Highlight search matches in the terminal grid (M4b Task 3)."
```

---

## Exit Criteria

- `Cmd+F` opens a non-modal search bar; typing filters live against the focused pane's full grid
  (visible + scrollback); every match highlights, current match visually distinct; Up/Down/Enter
  navigate; Escape closes and clears highlights.
- `Mux::search_active_terminal`/`Mux::filter_matches` keep their exact existing behavior and API,
  now backed by `crate::term::search`'s free functions that `gpui_shell` calls directly.
- `scripts/ci-local.sh` is green and the full `cargo test --lib` suite passes after each task.
- This completes M4b. M4c (context menu, scoped down) and M4d (toasts) remain, each to get its own
  plan under the same M4 spec.
