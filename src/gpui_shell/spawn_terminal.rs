// `spawn_terminal`/`spawn_terminal_at`: shell + PTY + grid construction.

use std::rc::Rc;
use std::sync::Arc;

use crate::app::pty_schedule::WakeupGate;
use crate::config::Config;
use crate::term::Terminal;

use super::font_state;

/// Spawn one real terminal (shell + PTY + alacritty grid).
///
/// Uses a genuinely no-op `term::Wakeup` closure. gpui owns the real window
/// and event loop, and constructing a
/// `winit::event_loop::EventLoop` at all — even one that is never run — has
/// a process-global side effect on macOS: it registers itself as the
/// `NSApplication`'s delegate. gpui drives its own window/run loop through
/// that same shared, process-wide `NSApplication`, so the two conflict
/// fatally (a winit `EventLoop` half-installs a delegate it never actually
/// runs, and AppKit ends up routing an event through it, which panics: "a
/// delegate was not configured on the application"). No winit APIs must be
/// called anywhere in this module. Returns the terminal's `WakeupGate`
/// alongside it: `GpuiShellRoot`'s poll loop checks it each tick and only
/// calls `cx.notify()` when the PTY actually produced output, rather
/// than gpui's own cross-thread wake (no `spawn_blocking`-style bridge
/// exists in gpui 0.2.2's `BackgroundExecutor` to drive that from here).
///
/// Cell pixel size for the PTY winsize comes from
/// `font_state::measured_cell_size()` -- the same font-metrics-driven value
/// the render path uses.
pub(crate) fn spawn_terminal(
    cols: u16,
    rows: u16,
    config: &Config,
) -> anyhow::Result<(Rc<Terminal>, Arc<WakeupGate>)> {
    spawn_terminal_at(cols, rows, config, None)
}

/// Same as `spawn_terminal`, but spawns the shell in `cwd` instead of the
/// process's own working directory -- used by workspace restore
/// (`workspace_snapshot.rs`) to recreate panes at their saved CWDs.
pub(crate) fn spawn_terminal_at(
    cols: u16,
    rows: u16,
    config: &Config,
    cwd: Option<std::path::PathBuf>,
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
        cwd,
    )?;
    Ok((Rc::new(terminal), wakeup_gate))
}
