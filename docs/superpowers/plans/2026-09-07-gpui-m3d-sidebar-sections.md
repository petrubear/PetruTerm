# gpui M3d: Sidebar MCP/Skills/Steering Sections + InfoOverlay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the M3c workspace sidebar into the full 4-section drawer the M3 design promised: Workspaces (already done), MCP, Skills, and Steering — each a read-only browser of an existing engine-agnostic manager, opening a scrollable `InfoOverlay` popup on activation, navigable by both mouse click and keyboard (Tab cycles sections, arrows/j-k move a cursor, Enter activates, Escape closes).

**Architecture:** Port three tiny engine-agnostic managers (`SkillManager`/`SteeringManager`/`McpManager`, ~540 lines combined, already used verbatim by the wgpu build) directly into `GpuiShellRoot`, same "used directly, never copied" relationship M3b established for `ChatPanel`. Add a new `InfoOverlay` gpui component (a true modal popup, using gpui's real `ScrollHandle` rather than porting the wgpu build's manual line-offset scrolling). Give the sidebar its own `FocusHandle` and a dual click/keyboard interaction model — a genuinely new pattern for this codebase, approved by the user over the wgpu build's keyboard-only original.

**Tech Stack:** Rust, gpui 0.2.2 (existing `gpui_shell` conventions only — no new crates; every manager this plan wires in already exists and is already a dependency).

**Spec:** `docs/superpowers/specs/2026-09-06-gpui-m3-sidebars-design.md` (§4 names `sidebar/` as the M3c/d home for this work; M3d's own row in the milestone table is otherwise thin — the design decisions below were derived by the controller from the wgpu reference (`src/app/mod.rs`, `src/app/ui/mod.rs`, `src/app/ui/providers.rs`, `src/llm/{skills,steering,mcp}.rs`, `src/ui/info_overlay.rs`) and approved by the user directly in conversation, not written to a separate spec file).

## Global Constraints

- 400-line module limit (split further if a task's file grows past it) — Tasks 3/4 proactively split `sidebar/render.rs` (chrome + dispatch) from a new `sidebar/sections.rs` (per-section list bodies) from the start, precisely because M3c's `actions.rs` grew past this limit by NOT planning a split up front.
- Tests for logic only — no painting/layout/hit-testing tests (those are dogfooded). GPU windows cannot be captured from the agent sandbox.
- `scripts/ci-local.sh` must stay green after every task (clippy `-D warnings` included — a field or method with no caller yet needs an explicit, narrowly-scoped `#[allow(dead_code)]` with a comment naming which later task removes it; this happened twice in M3c and is expected here too, called out per-task below rather than left for an implementer to rediscover).
- Commit format: `type: Message.` per `AGENTS.md`.
- **Key/focus guards key on real focus (`is_focused(window)`), never on visibility/open state** — the hard-won M3a/M3b/M3c rule. `InfoOverlay` is the one deliberate, documented exception (see Task 2): it is a genuine blocking modal (its backdrop calls `cx.stop_propagation()` on every click, so nothing behind it is reachable while it's visible), so `is_visible()` is provably correct there, not a shortcut — every other new guard in this plan (the sidebar's own) still keys on `is_focused(window)`.
- **Trust gating**: skill/steering project-local files and MCP project-local config are loaded only when `crate::llm::mcp::trust::is_trusted(&cwd)` returns true — port this exactly as the wgpu build does it (AUDIT-SEC-02/03 in that codebase's own history). Global (`~/.config/petruterm/...`) files always load regardless of trust.
- **MCP loading is synchronous at startup** (`tokio_rt.block_on(mgr.start_all(&cfg))`), matching the wgpu build's own `src/app/ui/mod.rs` exactly — this plan does not redesign it into an async poll-drain. Skip MCP entirely when `config.llm.enabled` is `false` (AUDIT-ENERGY-03 parity).
- **Interaction model** (user-approved): every sidebar row supports both a mouse click (immediate activation — switch for Workspaces, open `InfoOverlay` for the other three) and keyboard navigation while the sidebar holds focus (Tab/Shift+Tab cycles sections, ArrowDown/j and ArrowUp/k move a per-section cursor, Enter activates the cursor's row, Escape closes the drawer). The Workspaces section's keyboard cursor **is** its active-workspace index (arrow-nav switches immediately, same as a click) — a deliberate simplification over the wgpu reference's separate highlight-then-Enter two-step, since M3c's click already made switching immediate and a second, decoupled cursor would be confusing for one section only.

---

## Task 1: Wire `SkillManager`/`SteeringManager`/`McpManager` into `GpuiShellRoot`

**Purpose:** Pure plumbing — load the three managers at startup, store them on `GpuiShellRoot`. No UI, no keybinds yet (Task 4 is the first reader).

**Files:**
- Modify: `src/gpui_shell/mod.rs`

**Interfaces:**
- Consumes: `crate::llm::skills::SkillManager` (`new() -> Self`, `load(&mut self, cwd: &Path, include_local: bool)`, `skills(&self) -> &[SkillMeta]`, `match_query`/`read_body` — not used by this plan), `crate::llm::steering::SteeringManager` (`new()`, `load(...)`, `files(&self) -> &[(String, String)]`), `crate::llm::mcp::manager::McpManager` (`new()`, `async fn start_all(&mut self, config: &McpConfig) -> Vec<(String, anyhow::Error)>`, `all_tools(&self) -> Vec<(String, &McpTool)>`, `tools_for_server(&self, server_name: &str) -> &[McpTool]`, `connected_count(&self) -> usize`), `crate::llm::mcp::config::{load_global, load_local, McpConfig}`, `crate::llm::mcp::trust::is_trusted`.
- Produces: `GpuiShellRoot::{skill_manager: SkillManager, steering_manager: SteeringManager, mcp_manager: Arc<McpManager>}` — Task 4 is the first real reader; a `#[allow(dead_code)]` on each new field (removed in Task 4) keeps `cargo clippy -- -D warnings` green in the meantime, per this plan's Global Constraints note on that.

- [ ] **Step 1: Add imports**

In `src/gpui_shell/mod.rs`, add after the existing `use crate::term::Terminal;` line:

```rust
use crate::llm::mcp::manager::McpManager;
use crate::llm::mcp::{config as mcp_config, trust};
use crate::llm::skills::SkillManager;
use crate::llm::steering::SteeringManager;
```

- [ ] **Step 2: Add struct fields**

In the `GpuiShellRoot` struct, add after `ai_block: ai_block::AiBlockView,` (currently the last field):

```rust
    /// Skill metadata loaded from `~/.config/petruterm/skills/` (+ project-
    /// local, if trusted) at startup -- M3d's Skills sidebar section reads
    /// this directly, same "used by gpui_shell, never copied" relationship
    /// M3b already established for `ChatPanel`/`AiBlock`.
    #[allow(dead_code)] // first real reader is Task 4's Skills section
    skill_manager: SkillManager,
    /// Steering-file content loaded the same way, at the same time.
    #[allow(dead_code)] // first real reader is Task 4's Steering section
    steering_manager: SteeringManager,
    /// MCP server connections, started once at startup (mirrors the wgpu
    /// build's own blocking `tokio_rt.block_on(mgr.start_all(&cfg))`,
    /// `src/app/ui/mod.rs` -- ported as-is rather than redesigned into an
    /// async poll-drain, since the reference itself blocks app construction
    /// here and an LLM-disabled session skips this entirely). `Arc` because
    /// tool-calling (out of scope for M3d, a future milestone) would need to
    /// share it with a spawned async task the same way the wgpu build's own
    /// `mcp_manager` field does.
    #[allow(dead_code)] // first real reader is Task 4's MCP section
    mcp_manager: Arc<McpManager>,
```

- [ ] **Step 3: Hoist `tokio_rt` and load the three managers in `new()`**

Replace:

```rust
        let leader_map = leader::build_leader_map(
            &crate::config::keybind_view::leader_bindings_view(&config).bindings,
        );
        let chat = chat_panel::ChatPanelView::new(cx, &config);
        let ai_block = ai_block::AiBlockView::new(cx, &config);

        workspaces
            .active_mut()
            .tab_panes
            .push(PaneForest::new(terminal_id));

        Self {
            workspaces,
            terminals,
            next_terminal_id: terminal_id + 1,
            focus_handle: cx.focus_handle(),
            config,
            wakeup_gates,
            cursor_blink_on: true,
            cursor_last_blink: std::time::Instant::now(),
            rect_cache: Rc::new(RefCell::new(panes::RectCache::default())),
            leader_active: false,
            leader_deadline: None,
            resize_mode: false,
            leader_prefix: None,
            leader_map,
            // Same construction pattern as the wgpu app's own `tokio_rt`
            // field on its `App`/`Mux` struct (`src/app/ui/mod.rs`).
            tokio_rt: tokio::runtime::Runtime::new().expect("Failed to build tokio runtime"),
            cached_cwd: initial_cwd,
            git_branch: status_bar::GitBranchState::default(),
            exit_code: status_bar::ExitCodeState::default(),
            tab_rename: None,
            workspace_rename: None,
            sidebar: sidebar::WorkspaceSidebar::default(),
            chat,
            ai_block,
        }
    }
}
```

with:

```rust
        let leader_map = leader::build_leader_map(
            &crate::config::keybind_view::leader_bindings_view(&config).bindings,
        );
        let chat = chat_panel::ChatPanelView::new(cx, &config);
        let ai_block = ai_block::AiBlockView::new(cx, &config);

        // Same construction pattern as the wgpu app's own `tokio_rt` field
        // on its `App`/`Mux` struct (`src/app/ui/mod.rs`) -- hoisted into
        // its own binding, rather than built inline in the `Self { .. }`
        // literal below (M3c's shape), because MCP startup needs to
        // `.block_on()` it before the struct exists.
        let tokio_rt = tokio::runtime::Runtime::new().expect("Failed to build tokio runtime");

        // Skill/steering: load global (`~/.config/petruterm/{skills,steering}/`)
        // always; project-local (`<cwd>/.petruterm/{skills,steering}/`) only when
        // the cwd has been explicitly trusted -- mirrors the wgpu build's own
        // startup sequence (`src/app/ui/mod.rs`) exactly, including its
        // AUDIT-SEC-03 reasoning (a malicious repo's `.petruterm/` must not be
        // read just for being opened).
        let mut skill_manager = SkillManager::new();
        let mut steering_manager = SteeringManager::new();
        if let Ok(cwd) = std::env::current_dir() {
            let trusted = trust::is_trusted(&cwd);
            skill_manager.load(&cwd, trusted);
            steering_manager.load(&cwd, trusted);
        }

        // MCP: skip entirely when LLM is disabled -- no AI panel, no tool
        // calls (AUDIT-ENERGY-03, matching the wgpu build's own gate).
        // Project-local `.petruterm/mcp.json` is loaded only when trusted
        // (AUDIT-SEC-02): an untrusted repo's MCP config must not spawn
        // arbitrary processes just for being opened.
        let mcp_manager = if config.llm.enabled {
            let mut mgr = McpManager::new();
            if let Ok(mut cfg) = mcp_config::load_global() {
                if let Ok(cwd) = std::env::current_dir() {
                    let local_path = cwd.join(".petruterm/mcp.json");
                    if local_path.exists() && trust::is_trusted(&cwd) {
                        if let Ok(local) = mcp_config::load_local(&cwd) {
                            cfg.extend(local);
                        }
                    }
                }
                if !cfg.is_empty() {
                    let errors = tokio_rt.block_on(mgr.start_all(&cfg));
                    for (name, err) in &errors {
                        log::warn!("MCP server '{name}' failed to start: {err:#}");
                    }
                }
            }
            Arc::new(mgr)
        } else {
            Arc::new(McpManager::new())
        };

        workspaces
            .active_mut()
            .tab_panes
            .push(PaneForest::new(terminal_id));

        Self {
            workspaces,
            terminals,
            next_terminal_id: terminal_id + 1,
            focus_handle: cx.focus_handle(),
            config,
            wakeup_gates,
            cursor_blink_on: true,
            cursor_last_blink: std::time::Instant::now(),
            rect_cache: Rc::new(RefCell::new(panes::RectCache::default())),
            leader_active: false,
            leader_deadline: None,
            resize_mode: false,
            leader_prefix: None,
            leader_map,
            tokio_rt,
            cached_cwd: initial_cwd,
            git_branch: status_bar::GitBranchState::default(),
            exit_code: status_bar::ExitCodeState::default(),
            tab_rename: None,
            workspace_rename: None,
            sidebar: sidebar::WorkspaceSidebar::default(),
            chat,
            ai_block,
            skill_manager,
            steering_manager,
            mcp_manager,
        }
    }
}
```

- [ ] **Step 4: Build, test, verify**

Run: `cargo build 2>&1 | tail -60` — zero errors, zero warnings (the three `#[allow(dead_code)]` attributes suppress exactly the fields that would otherwise warn).

Run: `cargo test --lib 2>&1 | tail -10` — 228/228 passing, unchanged (no new logic, no new tests — this is pure startup wiring of already-tested managers).

Run: `./scripts/ci-local.sh` — must exit 0.

Dogfood: launch the app. It must start exactly as before (no new visible surface yet) — confirms the new startup work doesn't hang, crash, or slow launch unacceptably (MCP's blocking connect only matters if the user's `~/.config/petruterm/mcp.json` / global MCP config actually configures servers; with none configured, `cfg.is_empty()` skips the blocking call entirely).

- [ ] **Step 5: Commit**

```bash
git add src/gpui_shell/mod.rs
git commit -m "feat: Wire SkillManager/SteeringManager/McpManager into GpuiShellRoot (M3d Task 1)."
```

---

## Task 2: `InfoOverlay` — a scrollable, modal content popup

**Purpose:** The read-only content viewer every section's activation opens. Ported fresh (not copied) from `src/ui/info_overlay.rs`'s tiny data model, using gpui's real `ScrollHandle` instead of the wgpu build's manual line-offset scrolling. No caller wires into it yet (Task 3/4's job) except its own Escape/scroll key handling, which this task adds directly.

**Files:**
- Create: `src/gpui_shell/info_overlay.rs`
- Modify: `src/gpui_shell/chat_panel/mod.rs` (widen one module's visibility)
- Modify: `src/gpui_shell/mod.rs` (register module, add field, construct it)
- Modify: `src/gpui_shell/render.rs` (wire as a window-wide overlay)
- Modify: `src/gpui_shell/input.rs` (its own modal key guard)

**Interfaces:**
- Consumes: `crate::llm::markdown::{parse_markdown, AnnotatedLine, ParseState}` (already a dependency, used by `chat_panel`), `super::chat_panel::markdown::render_line` (existing, made reachable by Step 2 below), `gpui::ScrollHandle`.
- Produces: `pub struct InfoOverlay` with `is_visible(&self) -> bool`, `open(&mut self, title: String, content: &str)`, `close(&mut self)`, `scroll_down(&mut self)`, `scroll_up(&mut self)`; `pub fn render_info_overlay(overlay: &InfoOverlay, colors: &ColorScheme) -> Div` — Task 4's sidebar-row activation is the first real caller of `open`.

- [ ] **Step 1: Widen `chat_panel`'s `markdown` module visibility**

In `src/gpui_shell/chat_panel/mod.rs`, change:

```rust
mod markdown;
```

to:

```rust
pub(super) mod markdown;
```

(One line. `render_line`'s own `pub fn` was already public *within* `chat_panel`; `mod markdown;` being private meant only `chat_panel`'s own descendants could reach it at all. `pub(super)` makes it visible throughout `gpui_shell` — its parent module and everything else inside `gpui_shell`, `info_overlay.rs` included — without exposing it outside the crate's `gpui_shell` tree.)

- [ ] **Step 2: Write `src/gpui_shell/info_overlay.rs`**

```rust
// gpui chrome migration (M3d Task 2): a scrollable, read-only content
// popup -- the activation target for every sidebar row this milestone
// adds (MCP server, skill, steering file) and, later, workspace details.
//
// Ported fresh from `src/ui/info_overlay.rs`'s tiny data model (title +
// parsed-markdown lines + scroll position), not copied verbatim: that
// version hand-rolls scroll as a `usize` line offset because the wgpu
// renderer has no real scroll container to delegate to. gpui does --
// `gpui::ScrollHandle` plus `.track_scroll()`/`.scroll_to_item()` -- so
// this version drives a real one instead of reimplementing scrolling.
//
// Deliberately modal, and deliberately the ONE guard in this codebase keyed
// on visibility instead of focus (see `input.rs`'s own doc comment on its
// guard for the full reasoning): its backdrop calls `cx.stop_propagation()`
// on every click (Step 4), so nothing behind it is reachable while it's
// open -- unlike the chat panel or AI block, which are deliberately
// non-modal and let the terminal stay interactive underneath them.

use gpui::{div, prelude::*, px, rgba, App, Div, FontWeight, MouseButton, MouseDownEvent, ScrollHandle, Window};

use crate::config::schema::ColorScheme;
use crate::llm::markdown::{parse_markdown, AnnotatedLine, ParseState};

use super::chat_panel::markdown::render_line;
use super::font_state;
use super::pane_view::to_rgba;

/// Character width `parse_markdown` wraps content to -- matches the wgpu
/// build's own `CONTENT_WIDTH` (`src/app/mod.rs`'s `open_sidebar_info_
/// overlay`): wide enough that tool-schema JSON and skill prose read
/// naturally, still narrower than most terminal windows.
const CONTENT_WIDTH: usize = 72;

pub struct InfoOverlay {
    visible: bool,
    title: String,
    lines: Vec<AnnotatedLine>,
    scroll_handle: ScrollHandle,
    /// Which line `scroll_to_item` targets on the next j/k/arrow press --
    /// gpui's `ScrollHandle` tracks pixel offset, not "the Nth line", so
    /// this is the source of truth for "where the keyboard cursor is",
    /// translated into a `scroll_to_item` call each time it moves.
    cursor_line: usize,
}

impl InfoOverlay {
    pub fn new() -> Self {
        Self {
            visible: false,
            title: String::new(),
            lines: Vec::new(),
            scroll_handle: ScrollHandle::new(),
            cursor_line: 0,
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Open the overlay showing `content` (parsed as markdown, reusing the
    /// same `AnnotatedLine` model the chat panel's message list already
    /// uses) under `title`.
    #[allow(dead_code)] // first caller is Task 4's sidebar row activation
    pub fn open(&mut self, title: String, content: &str) {
        let mut state = ParseState::default();
        self.lines = parse_markdown(content, CONTENT_WIDTH, &mut state);
        self.title = title;
        self.cursor_line = 0;
        self.scroll_handle = ScrollHandle::new();
        self.visible = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
    }

    pub fn scroll_down(&mut self) {
        let max = self.lines.len().saturating_sub(1);
        self.cursor_line = (self.cursor_line + 1).min(max);
        self.scroll_handle.scroll_to_item(self.cursor_line);
    }

    pub fn scroll_up(&mut self) {
        self.cursor_line = self.cursor_line.saturating_sub(1);
        self.scroll_handle.scroll_to_item(self.cursor_line);
    }
}

impl Default for InfoOverlay {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the overlay's `div()` tree: a dimmed, window-covering backdrop
/// (blocking all mouse interaction with anything behind it -- see this
/// module's own doc comment) centered on a fixed-size content box.
pub fn render_info_overlay(overlay: &InfoOverlay, colors: &ColorScheme) -> Div {
    let lines: Vec<_> = overlay
        .lines
        .iter()
        .map(|line| render_line(line, colors))
        .collect();

    div()
        .id("info-overlay-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x000000_99))
        .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _window: &mut Window, cx: &mut App| {
            // Swallow clicks on the backdrop itself so they can't leak
            // through to whatever is positioned underneath -- this call is
            // what makes `is_visible()` a provably correct keyboard-guard
            // signal in `input.rs` (see that guard's own doc comment).
            cx.stop_propagation();
        })
        .child(
            div()
                .id("info-overlay-content")
                .flex()
                .flex_col()
                .w(px(640.0))
                .h(px(480.0))
                .bg(to_rgba(colors.ui_surface))
                .border_1()
                .border_color(to_rgba(colors.ui_border))
                .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _window: &mut Window, cx: &mut App| {
                    // Clicks inside the content box (e.g. selecting text)
                    // must not also register as a backdrop click.
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(to_rgba(colors.ui_border))
                        .font_family(font_state::font_family())
                        .font_weight(FontWeight::BOLD)
                        .text_color(to_rgba(colors.foreground))
                        .child(overlay.title.clone()),
                )
                .child(
                    div()
                        .id("info-overlay-scroll")
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&overlay.scroll_handle)
                        .px_3()
                        .py_2()
                        .gap_1()
                        .font_family(font_state::font_family())
                        .text_size(px(font_state::font_size()))
                        .children(lines),
                ),
        )
}
```

- [ ] **Step 3: Register the module, add the field**

In `src/gpui_shell/mod.rs`, add `mod info_overlay;` to the module list, alphabetically after `mod input;` and before `mod key_map;`.

Add to the `GpuiShellRoot` struct, after the `mcp_manager` field from Task 1:

```rust
    /// The read-only content popup every sidebar row's activation opens
    /// (Task 4) -- see `info_overlay.rs`'s own doc comment for why it's
    /// modal and why that makes its `is_visible()` guard (`input.rs`)
    /// correct rather than a shortcut.
    info_overlay: info_overlay::InfoOverlay,
```

Add to the `Self { .. }` literal in `new()`, after `mcp_manager,`:

```rust
            info_overlay: info_overlay::InfoOverlay::new(),
```

- [ ] **Step 4: Wire it into `render()`**

In `src/gpui_shell/render.rs`, add `use super::info_overlay;` to the imports (alongside the existing `use super::sidebar;`).

Change the root div's `.flex()` call to add `.relative()` right before it (the info overlay's `.absolute()` backdrop needs a `.relative()` ancestor to anchor `.inset_0()` against — the whole window, in this case):

```rust
        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(to_rgba(self.config.colors.background))
            .child(tab_bar)
            .child(middle_row)
            .when_some(status_bar_row, |el, bar| el.child(bar))
            .when(self.info_overlay.is_visible(), |el| {
                el.child(info_overlay::render_info_overlay(
                    &self.info_overlay,
                    &self.config.colors,
                ))
            })
    }
}
```

(Replace the function's existing closing `div()...when_some(...)` block, which currently ends the function, with this version — the only changes are adding `.relative()` after `.on_key_down(...)` and appending the new `.when(self.info_overlay.is_visible(), ...)` call after `.when_some(status_bar_row, ...)`.)

- [ ] **Step 5: `InfoOverlay`'s own key guard in `input.rs`**

In `src/gpui_shell/input.rs`, add this as the very first statement inside `on_key_down`, before the existing `tab_rename` guard:

```rust
        // InfoOverlay intercepts all keys while open (Escape closes; Arrow
        // Down/Up or j/k scroll) -- checked first, before every other guard
        // in this function, since it visually sits on top of everything
        // else. Unlike every other guard here, this one is keyed on
        // `is_visible()`, not `is_focused(window)`: InfoOverlay grabs no
        // `FocusHandle` of its own (it's read-only -- nothing to type into,
        // nowhere for real gpui focus to go), and it is a genuine blocking
        // modal (its backdrop's `cx.stop_propagation()`, `info_overlay.rs`,
        // means there is no "clicked back into the terminal while this
        // stays open" case the way there is for the chat panel or AI
        // block) -- so visibility really is the only, and the correct,
        // signal here. See `info_overlay.rs`'s own doc comment for the
        // full reasoning.
        if self.info_overlay.is_visible() {
            match event.keystroke.key.as_str() {
                "escape" => self.info_overlay.close(),
                "down" => self.info_overlay.scroll_down(),
                "up" => self.info_overlay.scroll_up(),
                "j" => self.info_overlay.scroll_down(),
                "k" => self.info_overlay.scroll_up(),
                _ => {}
            }
            cx.notify();
            return;
        }

```

- [ ] **Step 6: Build, test, verify**

Run: `cargo build 2>&1 | tail -60`, `cargo test --lib 2>&1 | tail -10` (228/228, no new tests — no new testable logic, this task is UI + wiring), `./scripts/ci-local.sh` (exit 0).

Dogfood: nothing new is reachable yet (no caller opens the overlay) — confirm the app still launches and behaves exactly as before. This task's own correctness (does the popup actually render, scroll, and close correctly) is dogfooded together with Task 4, once something can open it.

- [ ] **Step 7: Commit**

```bash
git add src/gpui_shell/info_overlay.rs src/gpui_shell/chat_panel/mod.rs src/gpui_shell/mod.rs src/gpui_shell/render.rs src/gpui_shell/input.rs
git commit -m "feat: Add the InfoOverlay content popup (M3d Task 2)."
```

---

## Task 3: Sidebar section-switching skeleton (focus model + Tab/arrow navigation)

**Purpose:** Give the sidebar its own keyboard-focus identity and a 4-section structure (Workspaces/MCP/Skills/Steering), with Tab cycling and arrow/j-k navigation working end-to-end — exercised against the Workspaces section, the only one with real content until Task 4. MCP/Skills/Steering render as visible-but-empty placeholders this task, so section-cycling is honestly dogfoodable now rather than invisible until Task 4.

**Files:**
- Modify: `src/gpui_shell/sidebar/mod.rs`
- Create: `src/gpui_shell/sidebar/sections.rs` (Workspaces section body, moved out of `render.rs`)
- Modify: `src/gpui_shell/sidebar/render.rs` (becomes chrome + section-tab row + dispatcher)
- Modify: `src/gpui_shell/mod.rs` (new `sidebar_focus_handle` field)
- Modify: `src/gpui_shell/leader_dispatch.rs` (`ToggleWorkspaceSidebar` also moves focus)
- Modify: `src/gpui_shell/input.rs` (new sidebar-focus guard)
- Modify: `src/gpui_shell/render.rs` (thread the new context through)

**Interfaces:**
- Consumes: `WorkspaceManager` (M3c), `switch_workspace_to_index` (M3c, `actions.rs`).
- Produces: `sidebar::SidebarSection` enum (`Workspaces`/`Mcp`/`Skills`/`Steering`, with `next()`/`prev()`); `WorkspaceSidebar::{active_section() -> SidebarSection, next_section(&mut self), prev_section(&mut self), set_section(&mut self, SidebarSection)}`; `GpuiShellRoot::sidebar_focus_handle: FocusHandle`; `GpuiShellRoot::{sidebar_move_cursor(&mut self, delta: i32, cx: &mut Context<Self>), sidebar_activate_selection(&mut self, cx: &mut Context<Self>)}` (both `match self.sidebar.active_section() { Workspaces => .., _ => {} }` — the `_` arms are Task 4's to fill in); `sidebar::render::SidebarRenderCx<'a>` (bundles every render-time parameter; grows again in Task 4), `sidebar::render::SectionSelectCallback`.

- [ ] **Step 1: `sidebar/mod.rs` — section enum and state**

Replace the whole file:

```rust
// gpui chrome migration (M3c Task 4 / M3d Task 3): the workspace sidebar
// drawer's visibility, active-section, and per-section-cursor state.
// Mirrors `chat_panel`'s own `visible: bool` + `toggle`/`is_visible` shape
// (`chat_panel/mod.rs`) for the drawer-level state; the section/cursor
// fields are new in M3d.

pub mod render;
pub mod sections;

/// Which of the sidebar's four sections is active. Cycled by Tab/Shift+Tab
/// while the sidebar holds keyboard focus (`input.rs`), or by clicking a
/// section-tab label (`render.rs`'s `on_select_section`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarSection {
    #[default]
    Workspaces,
    Mcp,
    Skills,
    Steering,
}

impl SidebarSection {
    pub fn next(self) -> Self {
        match self {
            Self::Workspaces => Self::Mcp,
            Self::Mcp => Self::Skills,
            Self::Skills => Self::Steering,
            Self::Steering => Self::Workspaces,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Workspaces => Self::Steering,
            Self::Mcp => Self::Workspaces,
            Self::Skills => Self::Mcp,
            Self::Steering => Self::Skills,
        }
    }
}

#[derive(Default)]
pub struct WorkspaceSidebar {
    visible: bool,
    active_section: SidebarSection,
    /// Highlighted row within the MCP section's server list. Unused until
    /// Task 4 renders that list; kept here now so the section-switching
    /// skeleton this task builds doesn't need touching again to add it.
    #[allow(dead_code)] // first reader is Task 4
    mcp_cursor: usize,
    /// Highlighted row within the Skills section's list. See `mcp_cursor`.
    #[allow(dead_code)] // first reader is Task 4
    skills_cursor: usize,
    /// Highlighted row within the Steering section's list. See `mcp_cursor`.
    #[allow(dead_code)] // first reader is Task 4
    steering_cursor: usize,
}

impl WorkspaceSidebar {
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Force the drawer open -- used by `begin_workspace_rename` (M3c) so
    /// the rename editor (rendered inline in the sidebar row) is never
    /// focused while invisible.
    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn active_section(&self) -> SidebarSection {
        self.active_section
    }

    pub fn next_section(&mut self) {
        self.active_section = self.active_section.next();
    }

    pub fn prev_section(&mut self) {
        self.active_section = self.active_section.prev();
    }

    pub fn set_section(&mut self, section: SidebarSection) {
        self.active_section = section;
    }

    #[allow(dead_code)] // first reader is Task 4
    pub fn mcp_cursor(&self) -> usize {
        self.mcp_cursor
    }

    #[allow(dead_code)] // first reader is Task 4
    pub fn set_mcp_cursor(&mut self, idx: usize) {
        self.mcp_cursor = idx;
    }

    #[allow(dead_code)] // first reader is Task 4
    pub fn skills_cursor(&self) -> usize {
        self.skills_cursor
    }

    #[allow(dead_code)] // first reader is Task 4
    pub fn set_skills_cursor(&mut self, idx: usize) {
        self.skills_cursor = idx;
    }

    #[allow(dead_code)] // first reader is Task 4
    pub fn steering_cursor(&self) -> usize {
        self.steering_cursor
    }

    #[allow(dead_code)] // first reader is Task 4
    pub fn set_steering_cursor(&mut self, idx: usize) {
        self.steering_cursor = idx;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_next_cycles_through_all_four_and_wraps() {
        let mut s = SidebarSection::Workspaces;
        s = s.next();
        assert_eq!(s, SidebarSection::Mcp);
        s = s.next();
        assert_eq!(s, SidebarSection::Skills);
        s = s.next();
        assert_eq!(s, SidebarSection::Steering);
        s = s.next();
        assert_eq!(s, SidebarSection::Workspaces);
    }

    #[test]
    fn section_prev_cycles_backward_and_wraps() {
        let mut s = SidebarSection::Workspaces;
        s = s.prev();
        assert_eq!(s, SidebarSection::Steering);
        s = s.prev();
        assert_eq!(s, SidebarSection::Skills);
        s = s.prev();
        assert_eq!(s, SidebarSection::Mcp);
        s = s.prev();
        assert_eq!(s, SidebarSection::Workspaces);
    }
}
```

- [ ] **Step 2: Move the Workspaces section body into `sidebar/sections.rs`**

Create `src/gpui_shell/sidebar/sections.rs`:

```rust
// gpui chrome migration (M3d Task 3): per-section list bodies for the
// sidebar drawer. Workspaces (this task, moved out of `render.rs` to keep
// that file from repeating M3c's `actions.rs` mistake of growing past 400
// lines one small addition at a time) is the only one implemented here
// yet; Task 4 adds Mcp/Skills/Steering to this same file.

use std::rc::Rc;

use gpui::{div, prelude::*, App, Div, MouseButton, MouseDownEvent, Window};

use crate::config::schema::ColorScheme;

use super::super::pane_view::to_rgba;
use super::super::workspace::WorkspaceManager;

pub type WorkspaceSelectCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;
pub type WorkspaceNewCallback = Rc<dyn Fn(&(), &mut Window, &mut App)>;
pub type WorkspaceCloseCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

/// `rename`: `(workspace_id, editor)` -- same "pinned to an id, taken at
/// most once, placed on the matching row" shape as `tabs::render_tab_bar`'s
/// own `rename` parameter.
pub fn render_workspaces_section(
    workspaces: &WorkspaceManager,
    colors: &ColorScheme,
    on_select: WorkspaceSelectCallback,
    on_new: WorkspaceNewCallback,
    on_close: WorkspaceCloseCallback,
    rename: Option<(usize, gpui::AnyElement)>,
) -> Div {
    let active_index = workspaces.active_index();
    let mut rename = rename;
    let rows: Vec<_> = workspaces
        .workspaces()
        .iter()
        .enumerate()
        .map(|(idx, ws)| {
            let is_active = idx == active_index;
            let is_renaming = rename.as_ref().is_some_and(|(id, _)| *id == ws.id);
            let select = on_select.clone();
            let close = on_close.clone();
            let ws_id = ws.id;
            let label = format!("{}  ({} tabs)", ws.name, ws.tabs.tab_count());
            let row = div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px_2()
                .py_1()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    select(&idx, window, cx)
                })
                .when(is_renaming, |el| {
                    el.child(rename.take().expect("checked is_some").1)
                })
                .when(!is_renaming, |el| el.child(label))
                .child(
                    div()
                        .cursor_pointer()
                        .px_1()
                        .child("x")
                        .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                            close(&ws_id, window, cx)
                        }),
                );
            if is_active {
                row.bg(to_rgba(colors.ui_surface_active))
                    .text_color(to_rgba(colors.foreground))
            } else {
                row.text_color(to_rgba(colors.ui_muted))
            }
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px_2()
                .py_2()
                .border_b_1()
                .border_color(to_rgba(colors.ui_border))
                .child("Workspaces")
                .child(
                    div()
                        .cursor_pointer()
                        .px_1()
                        .child("+")
                        .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                            on_new(&(), window, cx)
                        }),
                ),
        )
        .children(rows)
}

/// A section with no content of its own yet -- Task 4 replaces every call
/// site of this with a real list renderer.
pub fn render_placeholder_section(name: &str, colors: &ColorScheme) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .p_2()
        .text_color(to_rgba(colors.ui_muted))
        .child(format!("{name}: not yet implemented."))
}
```

- [ ] **Step 3: `sidebar/render.rs` becomes chrome + section tabs + dispatcher**

Replace the whole file:

```rust
// gpui chrome migration (M3c Task 4 / M3d Task 3): the sidebar drawer's
// outer frame -- fixed width, background, section-tab row -- plus dispatch
// to whichever section's body (`sections.rs`) is active. Section bodies
// themselves moved to `sections.rs` in M3d Task 3 to keep this file
// focused and under the 400-line convention as Task 4 adds three more.

use std::rc::Rc;

use gpui::{div, prelude::*, px, App, Div, MouseButton, MouseDownEvent, Window};

use crate::config::schema::ColorScheme;

use super::super::font_state;
use super::super::pane_view::to_rgba;
use super::super::workspace::WorkspaceManager;
use super::sections::{
    render_placeholder_section, render_workspaces_section, WorkspaceCloseCallback,
    WorkspaceNewCallback, WorkspaceSelectCallback,
};
use super::SidebarSection;

pub const SIDEBAR_WIDTH_PX: f32 = 220.0;

pub type SectionSelectCallback = Rc<dyn Fn(&SidebarSection, &mut Window, &mut App)>;

/// Every render-time input the sidebar needs, bundled the same way
/// `pane_view::PaneRenderCx` bundles the pane tree's -- Task 4 adds the
/// MCP/Skills/Steering fields to this same struct.
pub struct SidebarRenderCx<'a> {
    pub workspaces: &'a WorkspaceManager,
    pub colors: &'a ColorScheme,
    pub active_section: SidebarSection,
    pub on_select_workspace: WorkspaceSelectCallback,
    pub on_new_workspace: WorkspaceNewCallback,
    pub on_close_workspace: WorkspaceCloseCallback,
    pub on_select_section: SectionSelectCallback,
    pub workspace_rename: Option<(usize, gpui::AnyElement)>,
}

pub fn render_workspace_sidebar(ctx: SidebarRenderCx<'_>) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .h_full()
        .w(px(SIDEBAR_WIDTH_PX))
        .bg(to_rgba(ctx.colors.ui_surface))
        .border_r_1()
        .border_color(to_rgba(ctx.colors.ui_border))
        .font_family(font_state::font_family())
        .text_size(px(font_state::font_size()))
        .child(render_section_tabs(
            ctx.active_section,
            ctx.colors,
            ctx.on_select_section,
        ))
        .child(match ctx.active_section {
            SidebarSection::Workspaces => render_workspaces_section(
                ctx.workspaces,
                ctx.colors,
                ctx.on_select_workspace,
                ctx.on_new_workspace,
                ctx.on_close_workspace,
                ctx.workspace_rename,
            ),
            SidebarSection::Mcp => render_placeholder_section("MCP", ctx.colors),
            SidebarSection::Skills => render_placeholder_section("Skills", ctx.colors),
            SidebarSection::Steering => render_placeholder_section("Steering", ctx.colors),
        })
}

fn render_section_tabs(
    active: SidebarSection,
    colors: &ColorScheme,
    on_select: SectionSelectCallback,
) -> Div {
    let tabs = [
        (SidebarSection::Workspaces, "Workspaces"),
        (SidebarSection::Mcp, "MCP"),
        (SidebarSection::Skills, "Skills"),
        (SidebarSection::Steering, "Steering"),
    ];
    let cells: Vec<_> = tabs
        .into_iter()
        .map(|(section, label)| {
            let is_active = section == active;
            let select = on_select.clone();
            let cell = div()
                .flex_1()
                .px_1()
                .py_1()
                .text_size(px(11.0))
                .cursor_pointer()
                .child(label)
                .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    select(&section, window, cx)
                });
            if is_active {
                cell.bg(to_rgba(colors.ui_surface_active))
                    .text_color(to_rgba(colors.foreground))
            } else {
                cell.text_color(to_rgba(colors.ui_muted))
            }
        })
        .collect();

    div()
        .flex()
        .flex_row()
        .flex_shrink_0()
        .border_b_1()
        .border_color(to_rgba(colors.ui_border))
        .children(cells)
}
```

- [ ] **Step 4: `mod.rs` — new focus handle**

Add to the `GpuiShellRoot` struct, right after the existing `sidebar: sidebar::WorkspaceSidebar,` field:

```rust
    /// The sidebar's own keyboard-focus identity, distinct from the root
    /// `focus_handle` -- lets `on_key_down` (`input.rs`) tell "the sidebar
    /// is open" (`sidebar.is_visible()`) apart from "the sidebar actually
    /// has keyboard focus right now" (`sidebar_focus_handle.is_focused
    /// (window)`), same distinction every other focusable surface in this
    /// codebase already needs (tab rename, workspace rename, the chat
    /// composer, the AI block).
    sidebar_focus_handle: FocusHandle,
```

Add to the `Self { .. }` literal in `new()`, right after `sidebar: sidebar::WorkspaceSidebar::default(),`:

```rust
            sidebar_focus_handle: cx.focus_handle(),
```

- [ ] **Step 5: `leader_dispatch.rs` — `ToggleWorkspaceSidebar` moves focus**

Replace:

```rust
            LeaderAction::ToggleWorkspaceSidebar => self.sidebar.toggle(),
```

with:

```rust
            LeaderAction::ToggleWorkspaceSidebar => {
                self.sidebar.toggle();
                // Same division of labor `ToggleAiPanel` already has: opening
                // moves focus TO the sidebar so Tab/arrow navigation (Task 3)
                // works immediately; closing gives it back to the terminal
                // rather than leaving a stale focus target (`is_focused`
                // would otherwise report `true` for a handle nothing can see
                // or type into anymore).
                if self.sidebar.is_visible() {
                    window.focus(&self.sidebar_focus_handle);
                } else {
                    window.focus(&self.focus_handle);
                }
            }
```

- [ ] **Step 6: `input.rs` — the sidebar's own focus guard + two new helper methods**

Add this guard block right after the existing `workspace_rename` guard block (before the chat composer guard):

```rust
        // Same guard, same reasoning, for the workspace sidebar (M3d) --
        // keyed on `is_focused(window)`, never on `self.sidebar.is_
        // visible()`: the drawer can be open while the terminal holds focus
        // (the user clicked back into it), and a visibility-keyed guard
        // here would swallow every terminal keystroke in that state -- the
        // exact M3a-class Critical this project has now avoided three times
        // over by keying every guard like it on real focus.
        // `ToggleWorkspaceSidebar`'s dispatch arm (`leader_dispatch.rs`) is
        // the only place that moves focus TO this handle via the keyboard;
        // clicking a row or section tab (Task 4 / this task's own section-
        // tab clicks) must do the same explicitly, same fix `TextInput::
        // on_mouse_down` needed in M3a.
        if self.sidebar_focus_handle.is_focused(window) {
            match event.keystroke.key.as_str() {
                "tab" => {
                    if event.keystroke.modifiers.shift {
                        self.sidebar.prev_section();
                    } else {
                        self.sidebar.next_section();
                    }
                }
                "down" | "j" => self.sidebar_move_cursor(1, cx),
                "up" | "k" => self.sidebar_move_cursor(-1, cx),
                "enter" => self.sidebar_activate_selection(cx),
                "escape" => {
                    self.sidebar.toggle();
                    window.focus(&self.focus_handle);
                }
                _ => {}
            }
            cx.notify();
            return;
        }

```

Add two new methods to `impl GpuiShellRoot` in `input.rs`, after `on_key_down` (still inside the same `impl` block, i.e. before its closing `}`):

```rust
    /// Move the highlighted row within whichever section is active by
    /// `delta` (`+1`/`-1`). Only the Workspaces arm does anything yet --
    /// arrow-nav there switches immediately, same as a click (see this
    /// plan's Global Constraints on why that section's cursor IS its active
    /// index, unlike the wgpu reference's decoupled highlight-then-Enter).
    /// The other three arms are Task 4's to fill in, once their sections
    /// have real lists to move a cursor over.
    pub(super) fn sidebar_move_cursor(&mut self, delta: i32, cx: &mut Context<Self>) {
        match self.sidebar.active_section() {
            sidebar::SidebarSection::Workspaces => {
                let len = self.workspaces.len();
                if len == 0 {
                    return;
                }
                let current = self.workspaces.active_index() as i32;
                let next = (current + delta).rem_euclid(len as i32) as usize;
                self.switch_workspace_to_index(next);
            }
            sidebar::SidebarSection::Mcp
            | sidebar::SidebarSection::Skills
            | sidebar::SidebarSection::Steering => {
                // Task 4 fills these in once each section has a real list.
            }
        }
        cx.notify();
    }

    pub(super) fn sidebar_activate_selection(&mut self, cx: &mut Context<Self>) {
        match self.sidebar.active_section() {
            sidebar::SidebarSection::Workspaces => {
                // Arrow-nav already switched (above); a click already
                // switches too (`render.rs`'s `on_select_workspace`).
                // Nothing left for Enter to do.
            }
            sidebar::SidebarSection::Mcp
            | sidebar::SidebarSection::Skills
            | sidebar::SidebarSection::Steering => {
                // Task 4 fills these in: open the InfoOverlay for whichever
                // row `self.sidebar.{mcp,skills,steering}_cursor()` names.
            }
        }
        cx.notify();
    }
```

Add `use super::sidebar;` to `input.rs`'s existing imports (alongside `use super::leader::LeaderAction;` and `use super::{panes, GpuiShellRoot};`).

- [ ] **Step 7: `render.rs` — thread the new context through**

`use super::sidebar;` already exists in this file from M3c. Add one more line right after it: `use super::sidebar::SidebarSection;` (a bare, unqualified `SidebarSection` is used below in a closure's parameter type).

Replace the sidebar-callback-construction block (`let on_select_workspace: sidebar::render::WorkspaceSelectCallback = ...` through `let workspace_rename_element = ...`) — keep those four `let` bindings exactly as they are, and add one more callback right after `workspace_rename_element`'s binding:

```rust
        let on_select_section: sidebar::render::SectionSelectCallback =
            Rc::new(cx.listener(|this, section: &SidebarSection, window, cx| {
                this.sidebar.set_section(*section);
                window.focus(&this.sidebar_focus_handle);
                cx.notify();
            }));
```

Replace the `middle_row`'s sidebar-rendering `.when(...)` block:

```rust
            .when(self.sidebar.is_visible(), |el| {
                let bar = sidebar::render::render_workspace_sidebar(
                    &self.workspaces,
                    &self.config.colors,
                    on_select_workspace,
                    on_new_workspace,
                    on_close_workspace,
                    workspace_rename_element,
                );
                el.child(bar.with_animation(
                    "workspace-sidebar-drawer",
                    Animation::new(SIDEBAR_OPEN_ANIM).with_easing(ease_out_quint()),
                    |bar, delta| bar.w(px(sidebar::render::SIDEBAR_WIDTH_PX * delta)),
                ))
            })
```

with:

```rust
            .when(self.sidebar.is_visible(), |el| {
                let sidebar_ctx = sidebar::render::SidebarRenderCx {
                    workspaces: &self.workspaces,
                    colors: &self.config.colors,
                    active_section: self.sidebar.active_section(),
                    on_select_workspace,
                    on_new_workspace,
                    on_close_workspace,
                    on_select_section,
                    workspace_rename: workspace_rename_element,
                };
                let bar = sidebar::render::render_workspace_sidebar(sidebar_ctx);
                el.child(bar.with_animation(
                    "workspace-sidebar-drawer",
                    Animation::new(SIDEBAR_OPEN_ANIM).with_easing(ease_out_quint()),
                    |bar, delta| bar.w(px(sidebar::render::SIDEBAR_WIDTH_PX * delta)),
                ))
            })
```

- [ ] **Step 8: Build, test, verify**

Run: `cargo build 2>&1 | tail -80` (fix any borrow-checker issue in `sidebar_move_cursor`/`sidebar_activate_selection` by re-reading `switch_workspace_to_index`'s own signature in `actions.rs` — it takes `&mut self` and returns `bool`, called as a plain method, no special borrow shape needed here).

Run: `cargo test --lib 2>&1 | tail -10` — 228 + 2 new (`sidebar::tests::section_next_cycles...`, `section_prev_cycles...`) = 230 passing.

Run: `./scripts/ci-local.sh` — exit 0.

Dogfood: `Leader s` opens the sidebar and focuses it (confirm by immediately pressing an arrow key — it should move/switch the active workspace, proving keyboard focus landed there without an extra click). Tab cycles through all 4 section tabs (Workspaces → MCP → Skills → Steering → back to Workspaces); MCP/Skills/Steering show "`<name>`: not yet implemented." placeholders. Shift+Tab cycles backward. Click a section tab directly — switches immediately and grabs focus (confirm with an arrow key press right after clicking "MCP", say — Tab from there should move to "Skills", proving the click also focused the sidebar). ArrowDown/j and ArrowUp/k on the Workspaces section switch the active workspace, wrapping at both ends. Escape closes the sidebar and returns focus to the terminal (confirm by typing immediately after — it must land in the terminal, not vanish). Click into the terminal while the sidebar stays open (don't press Escape) — confirm typing there works normally and does NOT get intercepted by the sidebar guard (this is the guard's whole reason for being keyed on focus, not visibility).

- [ ] **Step 9: Commit**

```bash
git add src/gpui_shell/sidebar/ src/gpui_shell/mod.rs src/gpui_shell/leader_dispatch.rs src/gpui_shell/input.rs src/gpui_shell/render.rs
git commit -m "feat: Add sidebar section-switching with dual click/keyboard nav (M3d Task 3)."
```

---

## Task 4: MCP + Skills + Steering sections

**Purpose:** Replace the three placeholder sections with real list renderers, each row clickable and keyboard-activatable, opening `InfoOverlay` with that item's content. Same small shape three times — batched into one task per the SDD skill's "same-shape work" guidance.

**Files:**
- Modify: `src/gpui_shell/sidebar/sections.rs` (add three renderers, remove the placeholder)
- Modify: `src/gpui_shell/sidebar/render.rs` (dispatch to the three new renderers; extend `SidebarRenderCx`)
- Modify: `src/gpui_shell/render.rs` (build the three new callbacks + pass manager refs into `SidebarRenderCx`)
- Modify: `src/gpui_shell/input.rs` (fill in `sidebar_move_cursor`/`sidebar_activate_selection`'s Mcp/Skills/Steering arms; remove the `#[allow(dead_code)]`s from Task 1/2/3 that this task finally clears)
- Modify: `src/gpui_shell/mcp_overlay.rs` (new — the small `mcp_overlay_content` helper, ported from `src/app/ui/providers.rs`)

**Interfaces:**
- Consumes: `GpuiShellRoot::{skill_manager, steering_manager, mcp_manager}` (Task 1), `InfoOverlay::open` (Task 2), `WorkspaceSidebar::{mcp_cursor, skills_cursor, steering_cursor}` + their setters (Task 3), `SidebarRenderCx` (Task 3).
- Produces: `sections::{render_mcp_section, render_skills_section, render_steering_section}`; `mcp_overlay::mcp_overlay_content(mcp: &McpManager, server_name: &str) -> String`.

- [ ] **Step 1: `src/gpui_shell/mcp_overlay.rs` — port `mcp_overlay_content`**

```rust
// gpui chrome migration (M3d Task 4): builds the markdown content the info
// overlay shows for an MCP server -- ported verbatim from the wgpu build's
// `UiManager::mcp_overlay_content` (`src/app/ui/providers.rs`), as a free
// function instead of a method (gpui_shell has no `UiManager`).

use crate::llm::mcp::manager::McpManager;

pub fn mcp_overlay_content(mcp: &McpManager, server_name: &str) -> String {
    let tools = mcp.tools_for_server(server_name);
    let mut out = format!("# {server_name}\n\n");
    if tools.is_empty() {
        out.push_str("*No tools registered (server not connected or no tools).*\n");
        return out;
    }
    out.push_str(&format!("## Tools ({})\n\n", tools.len()));
    for tool in tools {
        out.push_str(&format!("### {}\n", tool.name));
        if !tool.description.is_empty() {
            out.push_str(&format!("{}\n\n", tool.description));
        }
        let schema = serde_json::to_string_pretty(&tool.input_schema).unwrap_or_default();
        if schema != "null" && !schema.is_empty() {
            out.push_str(&format!("```json\n{schema}\n```\n\n"));
        }
    }
    out
}
```

Register it in `src/gpui_shell/mod.rs`'s module list: `mod mcp_overlay;`, alphabetically after `mod mcp;`... there is no existing `mcp` module in `gpui_shell` (the real one lives at `crate::llm::mcp`), so place it alphabetically after `mod mouse;` — actually alphabetically `mcp_overlay` sits between `leader_dispatch` and `mouse` (`l` < `m` < `mo`... `mcp_overlay` vs `mouse`: "mc" < "mo", so `mcp_overlay` comes before `mouse`). Add `mod mcp_overlay;` right before `mod mouse;`.

- [ ] **Step 2: `sidebar/sections.rs` — the three new renderers**

Add these imports at the top of the file (alongside the existing ones):

```rust
use crate::llm::mcp::manager::McpManager;
use crate::llm::skills::SkillManager;
use crate::llm::steering::SteeringManager;
```

Remove `render_placeholder_section` (no longer called by anything after Step 3 below) and add:

```rust
pub type McpOpenCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;
pub type SkillOpenCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;
pub type SteeringOpenCallback = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

/// One row per connected MCP server (sorted by name, matching the wgpu
/// build's own sidebar render order in `src/app/mod.rs`'s `open_sidebar_
/// info_overlay`), showing its connected tool count. `cursor` highlights
/// the keyboard-nav row (Task 3's `sidebar_move_cursor`); clicking a row
/// opens it directly regardless of the cursor.
pub fn render_mcp_section(
    mcp: &McpManager,
    colors: &ColorScheme,
    cursor: usize,
    on_open: McpOpenCallback,
) -> Div {
    let mut servers: Vec<String> = {
        let mut set: std::collections::BTreeSet<String> = Default::default();
        for (server, _) in mcp.all_tools() {
            set.insert(server);
        }
        set.into_iter().collect()
    };
    servers.sort();

    if servers.is_empty() {
        return render_empty_section("No MCP servers connected.", colors);
    }

    let rows: Vec<_> = servers
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let tool_count = mcp.tools_for_server(name).len();
            let label = format!("{name}  ({tool_count} tools)");
            render_browser_row(label, idx == cursor, idx, on_open.clone(), colors)
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .children(rows)
}

/// One row per loaded skill, sorted the same way `SkillManager::skills()`
/// already returns them (load order: global first, then project-local
/// overlaying by name).
pub fn render_skills_section(
    skills: &SkillManager,
    colors: &ColorScheme,
    cursor: usize,
    on_open: SkillOpenCallback,
) -> Div {
    let metas = skills.skills();
    if metas.is_empty() {
        return render_empty_section("No skills loaded.", colors);
    }
    let rows: Vec<_> = metas
        .iter()
        .enumerate()
        .map(|(idx, skill)| {
            render_browser_row(skill.name.clone(), idx == cursor, idx, on_open.clone(), colors)
        })
        .collect();
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .children(rows)
}

/// One row per loaded steering file, `.md` suffix stripped for display
/// (matches `src/app/mod.rs`'s `open_sidebar_info_overlay`'s own `strip_
/// suffix(".md")`).
pub fn render_steering_section(
    steering: &SteeringManager,
    colors: &ColorScheme,
    cursor: usize,
    on_open: SteeringOpenCallback,
) -> Div {
    let files = steering.files();
    if files.is_empty() {
        return render_empty_section("No steering files loaded.", colors);
    }
    let rows: Vec<_> = files
        .iter()
        .enumerate()
        .map(|(idx, (name, _content))| {
            let label = name.strip_suffix(".md").unwrap_or(name).to_string();
            render_browser_row(label, idx == cursor, idx, on_open.clone(), colors)
        })
        .collect();
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .children(rows)
}

/// One clickable row, shared shape across all three browser sections above.
fn render_browser_row(
    label: String,
    is_cursor: bool,
    idx: usize,
    on_open: Rc<dyn Fn(&usize, &mut Window, &mut App)>,
    colors: &ColorScheme,
) -> Div {
    let row = div()
        .px_2()
        .py_1()
        .cursor_pointer()
        .child(label)
        .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
            on_open(&idx, window, cx)
        });
    if is_cursor {
        row.bg(to_rgba(colors.ui_surface_active))
            .text_color(to_rgba(colors.foreground))
    } else {
        row.text_color(to_rgba(colors.ui_muted))
    }
}

fn render_empty_section(message: &str, colors: &ColorScheme) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .p_2()
        .text_color(to_rgba(colors.ui_muted))
        .child(message.to_string())
}
```

- [ ] **Step 3: `sidebar/render.rs` — dispatch to the new sections, extend the context**

Replace the imports:

```rust
use super::sections::{
    render_placeholder_section, render_workspaces_section, WorkspaceCloseCallback,
    WorkspaceNewCallback, WorkspaceSelectCallback,
};
```

with:

```rust
use super::sections::{
    render_mcp_section, render_skills_section, render_steering_section, render_workspaces_section,
    McpOpenCallback, SkillOpenCallback, SteeringOpenCallback, WorkspaceCloseCallback,
    WorkspaceNewCallback, WorkspaceSelectCallback,
};
use crate::llm::mcp::manager::McpManager;
use crate::llm::skills::SkillManager;
use crate::llm::steering::SteeringManager;
```

Replace `SidebarRenderCx`'s definition:

```rust
pub struct SidebarRenderCx<'a> {
    pub workspaces: &'a WorkspaceManager,
    pub colors: &'a ColorScheme,
    pub active_section: SidebarSection,
    pub on_select_workspace: WorkspaceSelectCallback,
    pub on_new_workspace: WorkspaceNewCallback,
    pub on_close_workspace: WorkspaceCloseCallback,
    pub on_select_section: SectionSelectCallback,
    pub workspace_rename: Option<(usize, gpui::AnyElement)>,
    pub mcp_manager: &'a McpManager,
    pub mcp_cursor: usize,
    pub on_open_mcp: McpOpenCallback,
    pub skill_manager: &'a SkillManager,
    pub skills_cursor: usize,
    pub on_open_skill: SkillOpenCallback,
    pub steering_manager: &'a SteeringManager,
    pub steering_cursor: usize,
    pub on_open_steering: SteeringOpenCallback,
}
```

Replace the `match ctx.active_section { .. }` block inside `render_workspace_sidebar`:

```rust
        .child(match ctx.active_section {
            SidebarSection::Workspaces => render_workspaces_section(
                ctx.workspaces,
                ctx.colors,
                ctx.on_select_workspace,
                ctx.on_new_workspace,
                ctx.on_close_workspace,
                ctx.workspace_rename,
            ),
            SidebarSection::Mcp => render_mcp_section(
                ctx.mcp_manager,
                ctx.colors,
                ctx.mcp_cursor,
                ctx.on_open_mcp,
            ),
            SidebarSection::Skills => render_skills_section(
                ctx.skill_manager,
                ctx.colors,
                ctx.skills_cursor,
                ctx.on_open_skill,
            ),
            SidebarSection::Steering => render_steering_section(
                ctx.steering_manager,
                ctx.colors,
                ctx.steering_cursor,
                ctx.on_open_steering,
            ),
        })
```

- [ ] **Step 4: `render.rs` — build the three new callbacks, remove the Task-1 `#[allow(dead_code)]`s**

Add three new callback bindings in `render()`, right after `on_select_section`'s binding (from Task 3):

```rust
        let on_open_mcp: sidebar::sections::McpOpenCallback =
            Rc::new(cx.listener(|this, idx: &usize, _window, cx| {
                this.sidebar.set_mcp_cursor(*idx);
                this.sidebar_open_mcp_at(*idx);
                cx.notify();
            }));
        let on_open_skill: sidebar::sections::SkillOpenCallback =
            Rc::new(cx.listener(|this, idx: &usize, _window, cx| {
                this.sidebar.set_skills_cursor(*idx);
                this.sidebar_open_skill_at(*idx);
                cx.notify();
            }));
        let on_open_steering: sidebar::sections::SteeringOpenCallback =
            Rc::new(cx.listener(|this, idx: &usize, _window, cx| {
                this.sidebar.set_steering_cursor(*idx);
                this.sidebar_open_steering_at(*idx);
                cx.notify();
            }));
```

Extend the `SidebarRenderCx { .. }` literal built inside the `middle_row`'s `.when(self.sidebar.is_visible(), ...)` closure, adding these fields alongside the existing ones:

```rust
                    mcp_manager: &self.mcp_manager,
                    mcp_cursor: self.sidebar.mcp_cursor(),
                    on_open_mcp,
                    skill_manager: &self.skill_manager,
                    skills_cursor: self.sidebar.skills_cursor(),
                    on_open_skill,
                    steering_manager: &self.steering_manager,
                    steering_cursor: self.sidebar.steering_cursor(),
                    on_open_steering,
```

- [ ] **Step 5: `input.rs` — three new helper methods + fill in the cursor-move/activate arms**

Add three new methods to `impl GpuiShellRoot` (same `impl` block `sidebar_move_cursor`/`sidebar_activate_selection` live in):

```rust
    pub(super) fn sidebar_open_mcp_at(&mut self, idx: usize) {
        let mut servers: Vec<String> = {
            let mut set: std::collections::BTreeSet<String> = Default::default();
            for (server, _) in self.mcp_manager.all_tools() {
                set.insert(server);
            }
            set.into_iter().collect()
        };
        servers.sort();
        if let Some(name) = servers.get(idx) {
            let content = super::mcp_overlay::mcp_overlay_content(&self.mcp_manager, name);
            self.info_overlay.open(name.clone(), &content);
        }
    }

    pub(super) fn sidebar_open_skill_at(&mut self, idx: usize) {
        if let Some(skill) = self.skill_manager.skills().get(idx) {
            let title = skill.name.clone();
            let content = self
                .skill_manager
                .read_body(skill)
                .unwrap_or_else(|e| format!("Error reading skill: {e}"));
            self.info_overlay.open(title, &content);
        }
    }

    pub(super) fn sidebar_open_steering_at(&mut self, idx: usize) {
        if let Some((name, content)) = self.steering_manager.files().get(idx) {
            let display = name.strip_suffix(".md").unwrap_or(name).to_string();
            self.info_overlay.open(display, content);
        }
    }
```

Replace `sidebar_move_cursor`'s body (Task 3 left the Mcp/Skills/Steering arms empty):

```rust
    pub(super) fn sidebar_move_cursor(&mut self, delta: i32, cx: &mut Context<Self>) {
        match self.sidebar.active_section() {
            sidebar::SidebarSection::Workspaces => {
                let len = self.workspaces.len();
                if len == 0 {
                    return;
                }
                let current = self.workspaces.active_index() as i32;
                let next = (current + delta).rem_euclid(len as i32) as usize;
                self.switch_workspace_to_index(next);
            }
            sidebar::SidebarSection::Mcp => {
                let count = {
                    let mut set: std::collections::BTreeSet<String> = Default::default();
                    for (server, _) in self.mcp_manager.all_tools() {
                        set.insert(server);
                    }
                    set.len()
                };
                if count > 0 {
                    let current = self.sidebar.mcp_cursor() as i32;
                    let next = (current + delta).rem_euclid(count as i32) as usize;
                    self.sidebar.set_mcp_cursor(next);
                }
            }
            sidebar::SidebarSection::Skills => {
                let count = self.skill_manager.skills().len();
                if count > 0 {
                    let current = self.sidebar.skills_cursor() as i32;
                    let next = (current + delta).rem_euclid(count as i32) as usize;
                    self.sidebar.set_skills_cursor(next);
                }
            }
            sidebar::SidebarSection::Steering => {
                let count = self.steering_manager.files().len();
                if count > 0 {
                    let current = self.sidebar.steering_cursor() as i32;
                    let next = (current + delta).rem_euclid(count as i32) as usize;
                    self.sidebar.set_steering_cursor(next);
                }
            }
        }
        cx.notify();
    }
```

Replace `sidebar_activate_selection`'s body:

```rust
    pub(super) fn sidebar_activate_selection(&mut self, cx: &mut Context<Self>) {
        match self.sidebar.active_section() {
            sidebar::SidebarSection::Workspaces => {
                // Arrow-nav already switched; nothing left for Enter to do.
            }
            sidebar::SidebarSection::Mcp => self.sidebar_open_mcp_at(self.sidebar.mcp_cursor()),
            sidebar::SidebarSection::Skills => {
                self.sidebar_open_skill_at(self.sidebar.skills_cursor())
            }
            sidebar::SidebarSection::Steering => {
                self.sidebar_open_steering_at(self.sidebar.steering_cursor())
            }
        }
        cx.notify();
    }
```

- [ ] **Step 6: Remove the now-stale `#[allow(dead_code)]` attributes**

Remove `#[allow(dead_code)]` from: `GpuiShellRoot`'s `skill_manager`/`steering_manager`/`mcp_manager` fields (`mod.rs`, Task 1); `InfoOverlay::open` (`info_overlay.rs`, Task 2); `WorkspaceSidebar`'s `mcp_cursor`/`skills_cursor`/`steering_cursor` fields and their getter/setter methods (`sidebar/mod.rs`, Task 3) — every one of them now has a real caller. Run `cargo clippy --all-features -- -D warnings` after removing them; if any is still flagged, you missed a call site above — re-check rather than re-adding the attribute.

- [ ] **Step 7: Build, test, verify**

Run: `cargo build 2>&1 | tail -100`, `cargo test --lib 2>&1 | tail -10` (230 unchanged — no new pure-logic tests this task; every new function here is UI/wiring exercised by dogfood, matching this project's "no painting/hit-testing tests" convention), `cargo clippy --all-features -- -D warnings` (clean, confirming Step 6's cleanup was complete), `./scripts/ci-local.sh` (exit 0).

Dogfood, in order:
1. `Leader s`, Tab to MCP — if you have any MCP servers configured (`~/.config/petruterm/mcp.json` or a project-local one in a trusted directory), they list with tool counts; otherwise "No MCP servers connected." shows.
2. Click an MCP server row (or, with no mouse, arrow to it and press Enter) — `InfoOverlay` opens centered, dimmed backdrop, showing the server's tool list with JSON schemas in a code block. `j`/`k`/arrows scroll; Escape closes and returns keyboard control to the sidebar (confirm: after closing, Tab still cycles sections).
3. Tab to Skills — lists every skill from `~/.config/petruterm/skills/` (+ any trusted project-local ones); click or Enter opens its full body (frontmatter stripped, assets appended) in the overlay.
4. Tab to Steering — lists every `*.md` file from `~/.config/petruterm/steering/` (+ trusted project-local); click or Enter opens its content.
5. With the overlay open, click on the dimmed backdrop (not the content box) — confirm nothing behind it reacts (no stray click reaching the terminal or a sidebar row) and the overlay itself doesn't close (only Escape or Enter... wait, Enter has no effect while the overlay is open per this task's own guard — only Escape closes it; confirm that's the actual, intended behavior you want, not an oversight).
6. Click into the terminal while the sidebar is open on any section — confirm typing still works normally (the guard's `is_focused`, not `is_visible`, distinction, holding up under the exact scenario it exists for).

- [ ] **Step 8: Commit**

```bash
git add src/gpui_shell/mcp_overlay.rs src/gpui_shell/sidebar/ src/gpui_shell/render.rs src/gpui_shell/input.rs src/gpui_shell/mod.rs
git commit -m "feat: Add MCP/Skills/Steering sidebar sections (M3d Task 4)."
```

---

## Exit Criteria

- The sidebar has all four sections: Workspaces (M3c), MCP, Skills, Steering (this plan). Every section supports both a mouse click and Tab/arrow/Enter keyboard navigation to activate a row.
- `InfoOverlay` opens a scrollable, dimmed-backdrop modal popup for MCP/Skills/Steering row activation, scrolls via gpui's real `ScrollHandle`, and closes on Escape.
- Trust gating for project-local skills/steering/MCP config matches the wgpu build exactly (AUDIT-SEC-02/03 parity).
- `scripts/ci-local.sh` is green and the full `cargo test --lib` suite passes after each task.
- This closes out M3 entirely (M3a text input, M3b chat panel + inline AI block, M3c workspace layer + sidebar, M3d this plan) per the parent spec's milestone table.
