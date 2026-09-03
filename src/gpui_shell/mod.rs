// gpui chrome migration (M0 foundation spike + M1a foundation fixes): live
// terminal rendering root.
//
// Owns one or more `term::Terminal` instances and renders them through
// `TerminalGridElement` (see `terminal_element.rs`). Keyboard input is
// forwarded from gpui's key-down events straight to the PTY via `key_map`'s
// full key-event mapping.

pub mod font_state;
mod key_map;
pub mod terminal_element;

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use std::time::Duration;

use gpui::{
    actions, div, prelude::*, App, Context, FocusHandle, Focusable, KeyDownEvent, Render, Window,
};

use crate::app::pty_schedule::WakeupGate;
use crate::config::Config;
use crate::term::Terminal;
use terminal_element::TerminalGridElement;

// Proves gpui's native keymap dispatch (leader-key chorded matching) reaches
// real business logic by spawning a second live terminal. The keybinding
// itself is registered in `main` (see `src/bin/gpui_petruterm.rs`), per
// gpui's keymap-registration convention.
actions!(gpui_shell_spike, [SplitDemo]);

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

/// Shared between the config-watcher thread and `GpuiShellRoot`'s poll/wake
/// loop: `Some(config)` once a reload has happened and hasn't been applied
/// yet. `Mutex` because construction and consumption happen on different
/// threads; contention is negligible (checked at most ~30Hz, written only on
/// an actual file change).
static PENDING_CONFIG_RELOAD: Mutex<Option<Config>> = Mutex::new(None);
static CONFIG_CHANGED: AtomicBool = AtomicBool::new(false);

/// Spawn a dedicated thread running `ConfigWatcher`'s blocking watch loop
/// (the same notify-based watcher the wgpu `petruterm` binary uses), and
/// hand reloaded configs to `GpuiShellRoot`'s poll loop via
/// `PENDING_CONFIG_RELOAD`/`CONFIG_CHANGED` — gpui has no cross-thread wake
/// bridge in this version (see `spawn_terminal`'s doc comment), so pushing
/// data directly into gpui from this thread isn't an option.
///
/// Call exactly once, at startup (`main()`, alongside `terminal_element::
/// set_font_config`) — not from `GpuiShellRoot::new`. `PENDING_CONFIG_RELOAD`/
/// `CONFIG_CHANGED` are process-global statics; a second call (e.g. one per
/// window, if this app ever opens more than one) would spawn a second
/// watcher thread racing the first over the same slot, with no guarantee
/// either window's poll loop sees every update.
pub fn spawn_config_watcher() {
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

/// The `Render` root view for the gpui-petruterm spike window.
pub struct GpuiShellRoot {
    pub terminals: Vec<Rc<Terminal>>,
    pub focus_handle: FocusHandle,
    /// Index into `terminals` that keyboard input is routed to. M0 spike
    /// minimal: no click-to-focus, no visual indicator — just enough to
    /// prove a split terminal is independently live. Defaults to the
    /// most-recently-spawned terminal.
    active_terminal: usize,
    config: Config,
    wakeup_gates: Vec<Arc<WakeupGate>>,
}

impl GpuiShellRoot {
    pub fn new(cx: &mut Context<Self>, config: Config) -> Self {
        let (terminal, gate) = spawn_terminal(80, 24, &config).expect("spawn initial terminal");

        // M0 repaint-reliability stand-in (per the migration spec): PTY output
        // arrives on a background reader thread, decoupled from any gpui
        // entity/state mutation gpui itself would notice — without this,
        // nothing repaints the terminal grid until an unrelated event (e.g.
        // the next keystroke) incidentally triggers one, reproducing the
        // exact `gotcha_lost_pty_echo_wakeup`/Zed-vi-mode class of bug this
        // spike exists to catch. M0 called `cx.notify()` unconditionally on
        // every tick (measured ~21-24% idle CPU); M1a gates that on each
        // terminal's `WakeupGate` so a tick with no PTY activity since the
        // last check is a no-op — still up to 33ms repaint latency, but no
        // wasted relayout/repaint when nothing happened. gpui 0.2.2 has no
        // `spawn_blocking`-style bridge from `BackgroundExecutor` to drive a
        // true cross-thread wake instead (see `spawn_terminal`'s doc comment).
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;

                if CONFIG_CHANGED.swap(false, Ordering::AcqRel) {
                    if let Some(new_config) = PENDING_CONFIG_RELOAD.lock().unwrap().take() {
                        let font_config = new_config.font.clone();
                        // Apply the new config AND force a repaint unconditionally
                        // — a config change must show up even if no terminal has
                        // pending PTY output at this exact tick (the gate check
                        // below only fires on PTY activity, not config changes).
                        // reload_font_config needs `&mut App` (to drop the
                        // outgoing font's cached frames from the GPU sprite
                        // atlas, not just clear the Rust-side handles) — only
                        // available inside this closure via `cx`'s `DerefMut<
                        // Target = App>`, so the call lives here rather than
                        // before `this.update`.
                        let applied = this
                            .update(cx, |this: &mut Self, cx| {
                                font_state::reload_font_config(font_config, cx);
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

        Self {
            terminals: vec![terminal],
            focus_handle: cx.focus_handle(),
            active_terminal: 0,
            config,
            wakeup_gates: vec![gate],
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(terminal) = self.terminals.get(self.active_terminal) else {
            return;
        };
        let mode = terminal.with_term(|term| *term.mode());
        if let Some(bytes) =
            key_map::translate_key(&event.keystroke, mode, self.config.keyboard.option_as_meta)
        {
            terminal.write_input(&bytes);
            cx.notify();
        }
    }

    /// Demo action fired by the `ctrl-f %` chord (see `main`'s `cx.bind_keys`).
    /// Proves gpui's native keymap dispatch can reach real business logic:
    /// spawning a second live shell terminal side-by-side. Routes input to
    /// the newly spawned terminal so a human can verify it independently.
    fn on_split_demo(&mut self, _: &SplitDemo, _window: &mut Window, cx: &mut Context<Self>) {
        match spawn_terminal(80, 24, &self.config) {
            Ok((terminal, gate)) => {
                self.terminals.push(terminal);
                self.wakeup_gates.push(gate);
                self.active_terminal = self.terminals.len() - 1;
                cx.notify();
            }
            Err(e) => log::error!("gpui-shell spike: failed to spawn split terminal: {e:#}"),
        }
    }
}

impl Focusable for GpuiShellRoot {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for GpuiShellRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.focus(&self.focus_handle);
        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_action(cx.listener(Self::on_split_demo))
            .flex()
            .size_full()
            .children(self.terminals.iter().map(|t| {
                let (cell_width, cell_height) = font_state::measured_cell_size();
                TerminalGridElement {
                    terminal: t.clone(),
                    cell_width,
                    cell_height,
                }
            }))
    }
}
