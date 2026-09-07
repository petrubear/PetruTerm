// gpui chrome migration (M4a Task 3 review): `spawn_terminal`, split out of
// `mod.rs` for the 400-line convention -- `mod.rs` was already at 408 lines
// after M4a Task 1 landed (a review miss: build/test/clippy were checked at
// the time, but not `wc -l` on this specific file), the same class of
// overshoot `input.rs`/`render.rs` each hit once already in this milestone.
// A self-contained free function with no ties to `GpuiShellRoot`'s own
// struct/constructor is the cleanest thing to move out untouched.

use std::rc::Rc;
use std::sync::Arc;

use crate::app::pty_schedule::WakeupGate;
use crate::config::Config;
use crate::term::Terminal;

use super::font_state;

/// Spawn one real terminal (shell + PTY + alacritty grid).
///
/// Uses a genuinely no-op `term::Wakeup` closure. gpui owns the real window
/// and event loop in this migration, and constructing a
/// `winit::event_loop::EventLoop` at all — even one that is never run — has
/// a process-global side effect on macOS: it registers itself as the
/// `NSApplication`'s delegate. gpui drives its own window/run loop through
/// that same shared, process-wide `NSApplication`, so the two conflict
/// fatally (a winit `EventLoop` half-installs a delegate it never actually
/// runs, and AppKit ends up routing an event through it, which panics: "a
/// delegate was not configured on the application"). No winit APIs must be
/// called anywhere in this module. Returns the terminal's `WakeupGate`
/// alongside it: `GpuiShellRoot`'s poll loop checks it each tick and only
/// calls `cx.notify()` when the PTY actually produced output (M1a), rather
/// than gpui's own cross-thread wake (no `spawn_blocking`-style bridge
/// exists in gpui 0.2.2's `BackgroundExecutor` to drive that from here).
///
/// Cell pixel size for the PTY winsize comes from `terminal_element::
/// measured_cell_size()` — the same real, font-metrics-driven value the
/// render path uses (previously hardcoded `9, 18` here, disagreeing with
/// whatever the render path actually painted; both now read one cached
/// source of truth, kept in sync across config reloads).
pub(crate) fn spawn_terminal(
    cols: u16,
    rows: u16,
    config: &Config,
) -> anyhow::Result<(Rc<Terminal>, Arc<WakeupGate>)> {
    let (cell_width, cell_height) = font_state::measured_cell_size();
    let cell_w = f32::from(cell_width).round().max(1.0) as u16;
    let cell_h = f32::from(cell_height).round().max(1.0) as u16;
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
