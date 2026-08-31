# gpui Migration M1a (Foundation Fixes) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace M0's three foundational shortcuts — hardcoded font/config, a ~30Hz unconditional repaint poll, and hardcoded cell-size constants — with the real thing: the user's actual Lua config (with hot-reload), an event-driven (or at minimum idle-cheap) repaint mechanism, and font-measured cell metrics.

**Architecture:** All changes are within `src/gpui_shell/` (`mod.rs`, `terminal_element.rs`) and `src/bin/gpui_petruterm.rs`. No changes to the existing `petruterm` binary's own code paths. Each of the three fixes reuses existing, already-correct machinery from the rest of the codebase (`config::load()`/`ConfigWatcher`, `font::shaper`'s cell-measurement technique, `WakeupGate`) rather than inventing new mechanisms.

**Tech Stack:** gpui `0.2.2` (pinned, crates.io), cosmic-text `0.18` (existing dependency), the existing `config`/`font` modules (existing, proven code, untouched).

**Spec:** `docs/superpowers/specs/2026-08-30-gpui-chrome-migration-design.md` — see the `## M1a — Foundation Fixes: Design` section.

## Global Constraints

- Any new dependency must be the latest STABLE released version, exact-pinned, never a prerelease or git dependency — standing project rule (see `feedback_stable_deps_only` in memory; caused by the M0 `gpui` `main`-branch incident).
- This repo's CI treats warnings as hard errors. The real gate is `scripts/ci-local.sh` (`cargo clippy --all-features -- -D warnings`, `cargo fmt --check`, `cargo test --lib`, `cargo audit`) — NOT the narrower `cargo build`/`cargo test` substitute that let two real findings slip through M0 until its final review. Run `bash scripts/ci-local.sh` after every task, not just a plain build.
- Do not break the existing `petruterm` binary's compile/tests — nothing in this plan touches `src/main.rs`'s own module tree or its call sites; verify `cargo build --bin petruterm` and the full `cargo test` (134+ tests) stay green throughout.
- No custom test harness for GPU rendering/painting/timing — verified by human dogfood, per this migration's established testing philosophy. Unit tests are for real, pure business logic only (see each task).
- Every step that needs a human to look at a running window is a dogfood step: describe exactly what to run and what to look for, then stop and ask before continuing.
- Work happens on the existing `worktree-gpui-migration` branch/worktree (`.claude/worktrees/gpui-migration`) — do not create a new worktree, do not touch `master`.

---

### Task 1: Load the real config at startup (no hot-reload yet)

**Files:**
- Modify: `src/bin/gpui_petruterm.rs`
- Modify: `src/gpui_shell/mod.rs`
- Modify: `src/gpui_shell/terminal_element.rs`

**Interfaces:**
- Consumes: `petruterm::config::load() -> anyhow::Result<(Config, mlua::Lua)>` (existing, in `src/config/mod.rs`).
- Produces: `GpuiShellRoot::new(cx: &mut Context<Self>, config: Config) -> Self` (signature change — was `new(cx: &mut Context<Self>)`); `spawn_terminal(cols: u16, rows: u16, cell_w: u16, cell_h: u16, config: &Config) -> anyhow::Result<Rc<Terminal>>` (signature change — was without the `config` param); a module-level `static FONT_CONFIG: std::sync::OnceLock<crate::config::schema::FontConfig>` in `terminal_element.rs`, set once via `terminal_element::set_font_config(font_config: FontConfig)` (new `pub` function) before any window/element exists.

- [ ] **Step 1: Add `set_font_config` and the backing `OnceLock` to `terminal_element.rs`**

`FONT_SYSTEM`'s `thread_local!` initializer currently hardcodes `Config::default()` plus an inline family override — it can't take a runtime parameter directly (thread-local initializers are argument-less closures). Route the real font config through a `OnceLock` set once at startup instead.

In `src/gpui_shell/terminal_element.rs`, add near the top (after the existing `use` block):

```rust
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
```

- [ ] **Step 2: Replace `FONT_SYSTEM`'s hardcoded config with a read from `FONT_CONFIG`**

Replace this block in `terminal_element.rs` (the `thread_local! { static FONT_SYSTEM: ... }` initializer):

```rust
    static FONT_SYSTEM: RefCell<(FontSystem, String)> = RefCell::new({
        // `Config::default()`'s bare font family ("JetBrainsMono Nerd Font
        // Mono") is a code-level fallback, not what's actually configured —
        // the real app loads the user's Lua config (`config/default/*.lua`,
        // MonoLisaCode Nerd Font) at startup, which this M0 spike never
        // does. Override just the family/feature fields explicitly instead.
        let mut config = Config::default();
        config.font.family = "MonoLisaCode Nerd Font".into();
        let (font_system, actual_family, _face_id, _path, _face_index) =
            crate::font::loader::build_font_system(&config.font)
                .expect("load configured font for terminal ligature rendering");
        (font_system, actual_family)
    });
```

with:

```rust
    static FONT_SYSTEM: RefCell<(FontSystem, String)> = RefCell::new({
        let font_config = FONT_CONFIG
            .get()
            .expect("set_font_config must be called before the first paint");
        let (font_system, actual_family, _face_id, _path, _face_index) =
            crate::font::loader::build_font_system(font_config)
                .expect("load configured font for terminal ligature rendering");
        (font_system, actual_family)
    });
```

Remove the now-unused `use crate::config::Config;` import from this file if nothing else in it references `Config` directly (check — `spawn_terminal`'s config param, added in Step 4, lives in `mod.rs`, not here).

- [ ] **Step 3: Build and confirm the expected failure**

Run: `cargo build --bin gpui-petruterm 2>&1`

Expected: compiles (Steps 1-2 alone don't break anything — `FONT_CONFIG` is simply never populated yet, so the code would panic *at runtime* on first paint, not fail to compile). This is fine; Step 4 wires the caller.

- [ ] **Step 4: Thread `Config` through `spawn_terminal` and `GpuiShellRoot::new`**

In `src/gpui_shell/mod.rs`, change `spawn_terminal`'s signature and body:

```rust
pub fn spawn_terminal(
    cols: u16,
    rows: u16,
    cell_w: u16,
    cell_h: u16,
    config: &Config,
) -> anyhow::Result<Rc<Terminal>> {
    let wakeup: crate::term::Wakeup = Arc::new(|| {});
    let wakeup_gate = Arc::new(WakeupGate::new());
    let terminal = Terminal::new(
        config,
        cols,
        rows,
        cell_w,
        cell_h,
        wakeup,
        wakeup_gate,
        None,
    )?;
    Ok(Rc::new(terminal))
}
```

(Removes the `let config = Config::default();` line — the caller now supplies the real config.)

Change `GpuiShellRoot` to hold the config and thread it through:

```rust
pub struct GpuiShellRoot {
    pub terminals: Vec<Rc<Terminal>>,
    pub focus_handle: FocusHandle,
    active_terminal: usize,
    config: Config,
}

impl GpuiShellRoot {
    pub fn new(cx: &mut Context<Self>, config: Config) -> Self {
        let terminal = spawn_terminal(80, 24, 9, 18, &config).expect("spawn initial terminal");

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();

        Self {
            terminals: vec![terminal],
            focus_handle: cx.focus_handle(),
            active_terminal: 0,
            config,
        }
    }
```

And update `on_split_demo` (still in `mod.rs`) to pass `&self.config`:

```rust
    fn on_split_demo(&mut self, _: &SplitDemo, _window: &mut Window, cx: &mut Context<Self>) {
        match spawn_terminal(80, 24, 9, 18, &self.config) {
            Ok(terminal) => {
                self.terminals.push(terminal);
                self.active_terminal = self.terminals.len() - 1;
                cx.notify();
            }
            Err(e) => log::error!("gpui-shell spike: failed to spawn split terminal: {e:#}"),
        }
    }
```

- [ ] **Step 5: Load the real config in `main()` and call `set_font_config`**

Replace `src/bin/gpui_petruterm.rs` in full:

```rust
use gpui::{
    prelude::*, px, size, App, Application, Bounds, KeyBinding, WindowBounds, WindowOptions,
};
use petruterm::gpui_shell::{terminal_element, Backspace, GpuiShellRoot, SplitDemo};

fn main() {
    // Real user config (~/.config/petruterm/config.lua, falling back to the
    // embedded default) — the same function the wgpu app uses at startup.
    // No hot-reload yet (M1a Task 4 adds it); this replaces M0's hardcoded
    // `Config::default()` + inline font override.
    let (config, _lua) =
        petruterm::config::load().expect("load config for gpui-petruterm");
    terminal_element::set_font_config(config.font.clone());

    Application::new().run(move |cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("ctrl-f %", SplitDemo, None),
            KeyBinding::new("backspace", Backspace, None),
        ]);

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

(`terminal_element` must be `pub mod terminal_element;` in `src/gpui_shell/mod.rs` — it already is, per M0. Export `set_font_config` — already `pub fn` from Step 1, no further change needed.)

- [ ] **Step 6: Build, fix any remaining errors**

Run: `cargo build --bin gpui-petruterm 2>&1`

Fix anything that doesn't match exactly (e.g. if `Config` isn't `Clone` — it is, verified: `#[derive(Debug, Clone, Serialize, Deserialize)] pub struct Config` in `src/config/schema.rs`).

- [ ] **Step 7: Verify the full gate and the existing binary**

```bash
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```
All must pass clean; `cargo test` must show the same count as before this task (no regression).

- [ ] **Step 8: Dogfood checkpoint — STOP and ask the user to confirm**

Ask the user to run `cargo run --bin gpui-petruterm` and confirm the terminal still renders correctly with real text/ligatures (same as M0's final state) — this proves the real `config::load()` path produces the same font resolution as the hardcoded override did. If the user's `~/.config/petruterm/config.lua` sets a *different* font than MonoLisaCode Nerd Font, that's expected and correct — note whichever font actually renders.

- [ ] **Step 9: Commit**

```bash
git add src/bin/gpui_petruterm.rs src/gpui_shell/mod.rs src/gpui_shell/terminal_element.rs
git commit -m "[gpui-migration] feat: Load the real user config at startup (M1a).

Replaces M0's hardcoded Config::default() + inline font override with
the real config::load() path the wgpu app already uses. No hot-reload
yet (Task 4). FONT_SYSTEM's thread-local now reads font settings from
a OnceLock set once in main(), since thread-local initializers can't
take runtime parameters directly."
```

---

### Task 2: Real font-metrics-driven cell sizing

**Files:**
- Modify: `src/gpui_shell/terminal_element.rs`
- Modify: `src/gpui_shell/mod.rs`

**Interfaces:**
- Consumes: `FONT_SYSTEM`'s thread-local (Task 1), specifically the `(FontSystem, String)` tuple's `FontSystem` and `actual_family`.
- Produces: `terminal_element::measured_cell_size() -> (Pixels, Pixels)` (new `pub` function, returns `(cell_width, cell_height)`), replacing the `px(9.0)`/`px(18.0)` literals in `mod.rs`'s `TerminalGridElement { .. }` construction.

- [ ] **Step 1: Add a cell-measurement function to `terminal_element.rs`**

Port the measurement technique from `font::shaper::TextShaper::measure_cell()`'s fallback branch (shape a sample string via cosmic-text, read the resulting advance width) rather than importing `TextShaper` itself (that type is tied to the wgpu-oriented glyph-atlas machinery — see the spec's M1a design section for why). Add this function to `terminal_element.rs`, after the `FONT_SYSTEM`/`SWASH_CACHE`/`LAST_IMAGE` thread-locals:

```rust
/// Real cell width/height for the configured font at `FONT_SIZE`, measured by
/// shaping a sample string and reading its advance width — the same
/// technique `font::shaper::TextShaper::measure_cell()`'s fallback branch
/// uses, ported here rather than importing that (wgpu-atlas-coupled) type.
/// Computed fresh each call — cheap (one shape of a short string), and
/// Task 4 needs to be able to recompute this when the font changes on
/// config reload.
pub fn measured_cell_size() -> (Pixels, Pixels) {
    FONT_SYSTEM.with_borrow_mut(|(font_system, actual_family)| {
        let metrics = Metrics::new(FONT_SIZE, FONT_SIZE * 1.2);
        let mut buffer = Buffer::new(font_system, metrics);
        let mut buffer = buffer.borrow_with(font_system);
        buffer.set_size(Some(1000.0), Some(1000.0));

        let attrs = Attrs::new().family(Family::Name(actual_family.as_str()));
        // 16 `M`s, matching TextShaper::measure_cell's own sample — wide
        // enough for a stable average, short enough to stay off any line-wrap
        // boundary at this buffer width.
        buffer.set_text("MMMMMMMMMMMMMMMM", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(true);

        let run_width = buffer
            .layout_runs()
            .next()
            .map(|run| run.line_w)
            .unwrap_or(FONT_SIZE * 0.6 * 16.0);
        let cell_width = (run_width / 16.0).max(1.0);
        let cell_height = metrics.line_height.max(1.0);

        (px(cell_width), px(cell_height))
    })
}
```

**Verify `LayoutRun::line_w` and `Buffer::layout_runs()` are the correct cosmic-text 0.18 APIs before trusting this snippet** — check `https://raw.githubusercontent.com/pop-os/cosmic-text/f31b9d86959676d97fde54ff5907a58ab4308897/src/buffer.rs` (search for `layout_runs`) and `.../src/layout.rs` (search for `pub struct LayoutRun`, confirm the field name for the run's total shaped width — `line_w` is expected but not independently re-verified in this plan; M0's own research of this exact cosmic-text version verified adjacent APIs like `Buffer::draw`/`Metrics`/`Attrs` directly, but not `layout_runs`/`LayoutRun` specifically).

- [ ] **Step 2: Use it in `mod.rs`, replacing the hardcoded constants**

In `src/gpui_shell/mod.rs`, replace:

```rust
            .children(self.terminals.iter().map(|t| TerminalGridElement {
                terminal: t.clone(),
                cell_width: px(9.0),
                cell_height: px(18.0),
            }))
```

with:

```rust
            .children(self.terminals.iter().map(|t| {
                let (cell_width, cell_height) = terminal_element::measured_cell_size();
                TerminalGridElement {
                    terminal: t.clone(),
                    cell_width,
                    cell_height,
                }
            }))
```

(`px` import in `mod.rs` may become unused after this — check with the build in Step 3 and remove the import if so.)

- [ ] **Step 3: Build and fix errors**

```bash
cargo build --bin gpui-petruterm 2>&1
```

- [ ] **Step 4: Verify the full gate**

```bash
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```

- [ ] **Step 5: Dogfood checkpoint — STOP and ask the user to confirm**

Ask the user to run `cargo run --bin gpui-petruterm` and confirm: the terminal grid is correctly sized for the real font (not visibly too-wide or too-narrow per cell, matching M0's earlier finding that a mismatched hardcoded cell size made the cursor look oversized with a narrower font) — type some text and confirm cursor position tracks correctly under it.

- [ ] **Step 6: Commit**

```bash
git add src/gpui_shell/terminal_element.rs src/gpui_shell/mod.rs
git commit -m "[gpui-migration] feat: Measure real cell size from the font instead of hardcoded constants (M1a).

Ports the sample-shaping measurement technique from
font::shaper::TextShaper::measure_cell()'s fallback branch (not the
TextShaper type itself, which is coupled to the wgpu glyph-atlas).
Fixes the cursor-size/glyph-position drift M0's findings flagged as
the real cost behind 'the cursor renders oversized'."
```

---

### Task 3: Event-driven repaint (investigation + implementation)

This is the task with a genuine open question — resolve it empirically, don't guess. Time-box the investigation (Steps 1-2); if it doesn't resolve cleanly, use the fallback (Steps 3b) rather than spending unbounded effort on the ideal path.

**Files:**
- Modify: `src/gpui_shell/mod.rs`
- Modify: `src/app/pty_schedule.rs`

**Interfaces:**
- Consumes: `WakeupGate::signal() -> bool` and a new `WakeupGate::take_pending() -> bool` (added in Step 3b) — existing/new methods on the existing `pub(crate) struct WakeupGate` in `src/app/pty_schedule.rs`.
- Produces: `spawn_terminal(..., config: &Config) -> anyhow::Result<(Rc<Terminal>, Arc<WakeupGate>)>` (signature change — now also returns the gate); `GpuiShellRoot` gains a way to check all live terminals' gates each repaint tick.

- [ ] **Step 1: Investigate gpui's real cross-thread wake API (bounded — one research pass, not open-ended)**

Fetch and read, at the exact pinned commit (`69e2130295c2649963eb639fc70b4f2ee8ea1624`, matching `gpui = "=0.2.2"`):
```
https://raw.githubusercontent.com/zed-industries/zed/69e2130295c2649963eb639fc70b4f2ee8ea1624/crates/gpui/src/executor.rs
https://raw.githubusercontent.com/zed-industries/zed/69e2130295c2649963eb639fc70b4f2ee8ea1624/crates/gpui/src/app/async_context.rs
```
(if the second path 404s, list `https://api.github.com/repos/zed-industries/zed/contents/crates/gpui/src/app?ref=69e2130295c2649963eb639fc70b4f2ee8ea1624` and find the right file — likely named `async_context.rs` or similar, containing `AsyncApp`).

Look specifically for: does `BackgroundExecutor` (obtained via `cx.background_executor()`, already used in M0's poll loop) expose a way to run a **blocking** closure (not just `async fn`/`Future`) on a background thread and get a `Task<T>` back that can be `.await`ed from a `cx.spawn`'d async block? (Commonly named `spawn_blocking` or similar in executor APIs of this shape — verify the exact name/signature, don't assume.) If yes: this is the bridge — a `cx.spawn`'d task can call `cx.background_executor().<that method>(move || wakeup_gate.wait_for_signal_somehow())`, `.await` it, then call `this.update(cx, |_, cx| cx.notify())`, in a loop.

**If this exists and is straightforward:** implement it (Step 2). **If after this one research pass it's unclear, requires an unverified/undocumented API, or would need non-trivial new synchronization primitives:** stop investigating and use the fallback (Step 3b) instead — do not spend further rounds on the ideal path.

- [ ] **Step 2 (only if Step 1 found a clean API): implement the real cross-thread wake**

No code given here — this branch is genuinely contingent on Step 1's findings, which determine the exact API shape. Implement it following the pattern found, wire both the PTY `Wakeup` closure (currently `Arc::new(|| {})` in `spawn_terminal`) and skip Step 3b entirely. Verify with the same dogfood check as Step 4 below, then proceed to Step 5 (commit) — skip the fallback steps.

- [ ] **Step 3b (fallback, if Step 1 didn't resolve cleanly): smart poll using `WakeupGate`**

Add a consumer-side "check and clear" method to `WakeupGate` in `src/app/pty_schedule.rs` (it currently only has producer-side `signal()` and a clear-with-no-return `begin_drain()`):

```rust
    /// Consumer-side check-and-clear: returns whether a signal was pending,
    /// clearing it atomically. For a poll loop that only wants to act when
    /// something actually happened since the last check.
    pub(crate) fn take_pending(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }
```

Add a unit test alongside the existing ones in that file's `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn take_pending_clears_and_reports() {
        let gate = WakeupGate::new();
        assert!(!gate.take_pending());
        gate.signal();
        assert!(gate.take_pending());
        assert!(!gate.take_pending());
    }
```

In `src/gpui_shell/mod.rs`, change `spawn_terminal` to also return the gate (both call sites — `GpuiShellRoot::new` and `on_split_demo` — need updating):

```rust
pub fn spawn_terminal(
    cols: u16,
    rows: u16,
    cell_w: u16,
    cell_h: u16,
    config: &Config,
) -> anyhow::Result<(Rc<Terminal>, Arc<WakeupGate>)> {
    let wakeup: crate::term::Wakeup = Arc::new(|| {});
    let wakeup_gate = Arc::new(WakeupGate::new());
    let terminal = Terminal::new(
        config,
        cols,
        rows,
        cell_w,
        cell_h,
        wakeup,
        Arc::clone(&wakeup_gate),
        None,
    )?;
    Ok((Rc::new(terminal), wakeup_gate))
}
```

`GpuiShellRoot` tracks gates alongside terminals:

```rust
pub struct GpuiShellRoot {
    pub terminals: Vec<Rc<Terminal>>,
    pub focus_handle: FocusHandle,
    active_terminal: usize,
    config: Config,
    wakeup_gates: Vec<Arc<WakeupGate>>,
}

impl GpuiShellRoot {
    pub fn new(cx: &mut Context<Self>, config: Config) -> Self {
        let (terminal, gate) =
            spawn_terminal(80, 24, 9, 18, &config).expect("spawn initial terminal");

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                // One `update` call per tick: checks every terminal's gate
                // and notifies only if something actually happened, all
                // within the single closure that has `cx: &mut Context<Self>`
                // (matching M0's original `this.update(cx, |_, cx| cx.notify())`
                // shape exactly, just with the gate check added).
                let alive = this
                    .update(cx, |this: &mut Self, cx| {
                        if this.wakeup_gates.iter().any(|g| g.take_pending()) {
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !alive {
                    break; // window/entity gone
                }
            }
        })
        .detach();

        Self {
            terminals: vec![terminal],
            focus_handle: cx.focus_handle(),
            active_terminal: 0,
            config,
            wakeup_gates: vec![gate],
        }
    }
```

Update `on_split_demo`:

```rust
    fn on_split_demo(&mut self, _: &SplitDemo, _window: &mut Window, cx: &mut Context<Self>) {
        match spawn_terminal(80, 24, 9, 18, &self.config) {
            Ok((terminal, gate)) => {
                self.terminals.push(terminal);
                self.wakeup_gates.push(gate);
                self.active_terminal = self.terminals.len() - 1;
                cx.notify();
            }
            Err(e) => log::error!("gpui-shell spike: failed to spawn split terminal: {e:#}"),
        }
    }
```

The still-30Hz-timer loop now only calls `cx.notify()` when at least one terminal's gate actually has a pending signal — this doesn't reduce repaint *latency* (still up to 33ms), but eliminates the wasted `cx.notify()`/relayout/repaint cost on every tick where nothing happened, which is what M0's measured 21-24% idle CPU came from.

- [ ] **Step 4: Build, fix errors**

```bash
cargo build --bin gpui-petruterm 2>&1
```

- [ ] **Step 5: Verify the full gate**

```bash
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```
`cargo test` must include the new `take_pending_clears_and_reports` test (if the fallback path was taken) passing, plus the existing count unchanged otherwise.

- [ ] **Step 6: Dogfood checkpoint — STOP and ask the user to confirm, both correctness AND CPU**

Ask the user to:
1. Run `cargo run --bin gpui-petruterm`, type `sleep 2 && echo done`, don't touch anything — confirm `done` still appears on its own (repaint reliability, same check as M0).
2. While it's running and idle (no typing), check CPU usage (Activity Monitor, or `ps -o %cpu= -p <pid>` a few times over a few seconds) and report the rough idle CPU percentage — compare against M0's own measured ~21-24% baseline to confirm this task actually improved it.

- [ ] **Step 7: Commit**

If Step 1/2 (real cross-thread wake) was implemented:

```bash
git add src/gpui_shell/mod.rs src/app/pty_schedule.rs
git commit -m "[gpui-migration] feat: Real cross-thread event-driven repaint (M1a).

Replaces M0's unconditional ~30Hz poll with gpui's own cross-thread
wake mechanism (see commit body / code comments for the exact API
used) — PTY output now triggers a repaint directly, no polling."
```

If Step 3b (the `WakeupGate` smart-poll fallback) was implemented instead:

```bash
git add src/gpui_shell/mod.rs src/app/pty_schedule.rs
git commit -m "[gpui-migration] fix: Stop notifying gpui on every poll tick when nothing changed (M1a).

Bounded investigation into gpui's real cross-thread wake API (Step 1)
didn't turn up a clean, verified mechanism — using the designed
fallback instead: WakeupGate::take_pending() gates cx.notify() so the
still-~30Hz timer loop only repaints when a terminal actually produced
output, cutting M0's measured 21-24% idle CPU without solving the
harder cross-thread-async problem. Real event-driven wake stays a
real, named follow-up, not abandoned silently."
```

---

### Task 4: Config hot-reload

**Files:**
- Modify: `src/gpui_shell/mod.rs`
- Modify: `src/gpui_shell/terminal_element.rs`

**Interfaces:**
- Consumes: `config::watcher::ConfigWatcher::new(config_dir: &Path) -> anyhow::Result<Self>`, `.wait_timeout(&self, timeout: Duration) -> Option<PathBuf>` (existing, `src/config/watcher.rs`); `config::reload() -> anyhow::Result<(Config, mlua::Lua)>` (existing, `src/config/mod.rs`); `config::config_dir() -> PathBuf` (existing).
- Produces: `terminal_element::reload_font_config(font_config: FontConfig)` (new `pub` function — replaces `FONT_SYSTEM`'s cached contents, unlike `set_font_config` from Task 1 which only sets once).

- [ ] **Step 1: Make `terminal_element`'s font state reloadable, not just set-once**

`set_font_config`/`FONT_CONFIG` (Task 1) only support setting once (`OnceLock`). Hot-reload needs to *replace* the cached `FontSystem`/`actual_family` when the font changes. Add a new function to `terminal_element.rs`:

```rust
/// Rebuild `FONT_SYSTEM`'s cached font/family for a new config, on hot-reload.
/// Unlike `set_font_config` (set-once, for startup), this may be called
/// repeatedly. Also clears `LAST_IMAGE`'s cache — its entries are keyed on
/// content hash, not font identity, so a stale cache entry from the old font
/// would otherwise be served until content next changes.
pub fn reload_font_config(font_config: FontConfig) {
    let (new_font_system, new_family, _face_id, _path, _face_index) =
        match crate::font::loader::build_font_system(&font_config) {
            Ok(v) => v,
            Err(e) => {
                log::error!("gpui-shell: failed to reload font on config change: {e:#}");
                return;
            }
        };
    FONT_SYSTEM.with_borrow_mut(|(font_system, actual_family)| {
        *font_system = new_font_system;
        *actual_family = new_family;
    });
    LAST_IMAGE.with_borrow_mut(|cache| cache.clear());
}
```

(No `window.drop_image` calls needed for the cleared entries here — `LAST_IMAGE.clear()` drops the `Arc<RenderImage>` handles, but the GPU-side sprite atlas entries they reference are only freed via `window.drop_image`, which needs a live `&mut Window`, not available in this function's context. This is a small, deliberate scope boundary: the *next* paint after a font-reload will still correctly evict the old entry via the existing per-terminal `drop_image` logic in `paint()`, since the content hash won't match — the atlas leak fix from M0 already covers this case, just one paint cycle later than instantaneous. Note this in the commit message; it's not a new leak, the existing cache-eviction path handles it.)

- [ ] **Step 2: Build and verify Step 1 compiles**

```bash
cargo build --bin gpui-petruterm 2>&1
```

- [ ] **Step 3: Bridge `ConfigWatcher` into gpui**

In `src/gpui_shell/mod.rs`, add the watcher bridge to `GpuiShellRoot::new`. This reuses whatever wake mechanism Task 3 landed on (real cross-thread wake, or the smart-poll's `wakeup_gates` check) — for the config case specifically, since `ConfigWatcher` already blocks on its own dedicated thread, the simplest bridge regardless of Task 3's outcome is: spawn a plain `std::thread` running the watch loop, and on each change, update a `thread_local`-adjacent shared `Arc<Mutex<Option<Config>>>` (or reuse the `WakeupGate` pattern — a small dedicated `Arc<AtomicBool>` "config changed" flag checked by the same poll loop) rather than trying to push data directly into gpui from an arbitrary thread.

Concretely:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Shared between the config-watcher thread and `GpuiShellRoot`'s poll/wake
/// loop: `Some(config)` once a reload has happened and hasn't been applied
/// yet. `Mutex` because construction and consumption happen on different
/// threads; contention is negligible (checked at most ~30Hz, written only on
/// an actual file change).
static PENDING_CONFIG_RELOAD: Mutex<Option<Config>> = Mutex::new(None);
static CONFIG_CHANGED: AtomicBool = AtomicBool::new(false);

fn spawn_config_watcher() {
    std::thread::spawn(|| {
        let watcher = match crate::config::watcher::ConfigWatcher::new(&crate::config::config_dir())
        {
            Ok(w) => w,
            Err(e) => {
                log::error!("gpui-shell: failed to start config watcher: {e:#}");
                return;
            }
        };
        loop {
            if watcher
                .wait_timeout(std::time::Duration::from_secs(3600))
                .is_some()
            {
                match crate::config::reload() {
                    Ok((config, _lua)) => {
                        *PENDING_CONFIG_RELOAD.lock().unwrap() = Some(config);
                        CONFIG_CHANGED.store(true, Ordering::Release);
                    }
                    Err(e) => log::error!("gpui-shell: config reload failed: {e:#}"),
                }
            }
        }
    });
}
```

Call `spawn_config_watcher();` once at the top of `GpuiShellRoot::new`, before the existing `cx.spawn` poll/wake loop.

Replace the loop body from Task 3 (Step 3b's version — adjust equivalently if Step 1/2's real-wake path was taken instead, applying the same config-check addition before whatever notify call that path ends with) with this merged version, which checks for a pending config reload before the existing gate check, each tick:

```rust
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;

                if CONFIG_CHANGED.swap(false, Ordering::AcqRel) {
                    if let Some(new_config) = PENDING_CONFIG_RELOAD.lock().unwrap().take() {
                        let font_config = new_config.font.clone();
                        terminal_element::reload_font_config(font_config);
                        // Apply the new config AND force a repaint unconditionally
                        // — a config change must show up even if no terminal has
                        // pending PTY output at this exact tick (the gate check
                        // below only fires on PTY activity, not config changes).
                        let applied = this
                            .update(cx, |this: &mut Self, cx| {
                                this.config = new_config;
                                cx.notify();
                            })
                            .is_ok();
                        if !applied {
                            break; // window/entity gone
                        }
                    }
                }

                let alive = this
                    .update(cx, |this: &mut Self, cx| {
                        if this.wakeup_gates.iter().any(|g| g.take_pending()) {
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !alive {
                    break; // window/entity gone
                }
            }
        })
        .detach();
```

- [ ] **Step 4: Build, fix errors**

```bash
cargo build --bin gpui-petruterm 2>&1
```

- [ ] **Step 5: Verify the full gate**

```bash
bash scripts/ci-local.sh
RUSTFLAGS="-D warnings" cargo build --bin gpui-petruterm --bin petruterm 2>&1 | tail -60
cargo test 2>&1 | tail -20
```

- [ ] **Step 6: Dogfood checkpoint — STOP and ask the user to confirm**

Ask the user to:
1. Run `cargo run --bin gpui-petruterm`.
2. While it's running, edit `~/.config/petruterm/config.lua` and change the configured font family to something else installed on the system (or change `font.size`), save the file.
3. Confirm the running window picks up the change without restarting — text re-renders in the new font/size within a few seconds.
4. Revert the config file change afterward if it was just for this test.

- [ ] **Step 7: Commit**

```bash
git add src/gpui_shell/mod.rs src/gpui_shell/terminal_element.rs
git commit -m "[gpui-migration] feat: Config hot-reload for gpui-petruterm (M1a).

Bridges the existing ConfigWatcher (notify-based file watcher, already
used by the wgpu app) onto a dedicated thread; changes are picked up
by the same poll/wake loop Task 3 built and applied via
terminal_element::reload_font_config(), which rebuilds FONT_SYSTEM's
cached font and clears the LAST_IMAGE cache so stale-font frames
aren't served. Completes M1a's settings-wiring requirement (M0's
findings flagged font/features as hardcoded constants needing to
become real settings before the migration goes further)."
```

---

## M1a Exit Criteria

All three foundational fixes are dogfood-confirmed: the real Lua config drives the font (and updates live on file changes), the terminal grid is sized from real font metrics (not guessed constants), and repaint no longer wastes CPU on every idle tick (ideally event-driven; the `WakeupGate`-gated poll fallback is an acceptable, real fix if the ideal path didn't pan out in Task 3's bounded investigation). `scripts/ci-local.sh` passes clean after every task. M1a's completion unblocks M1b (cursor, selection, scrollback) and M1c (emoji, LCD AA), neither of which is designed yet — brainstorm each when its turn comes.
