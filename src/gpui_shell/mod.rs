// gpui chrome migration (M0 spike): live terminal rendering root.
//
// Owns one or more `term::Terminal` instances and renders them through
// `TerminalGridElement` (see `terminal_element.rs`). Keyboard input is
// forwarded from gpui's key-down events straight to the PTY.

pub mod terminal_element;

use std::rc::Rc;
use std::sync::Arc;

use std::time::Duration;

use gpui::{
    actions, div, prelude::*, px, App, Context, FocusHandle, Focusable, KeyDownEvent, Render,
    Window,
};

use crate::app::pty_schedule::WakeupGate;
use crate::config::Config;
use crate::term::Terminal;
use terminal_element::TerminalGridElement;

// M0 spike: proves leader-key chorded dispatch (gpui's native keymap matcher)
// reaches real business logic by spawning a second live terminal. The
// keybinding itself is registered in `main` (see `src/bin/gpui_petruterm.rs`),
// per gpui's keymap-registration convention.
//
// `Backspace` is a targeted fix for the one control key that blocked basic
// dogfooding (it has no `key_char`, so `on_key_down`'s minimal char-forwarding
// never sees it) — not a start on full key-event mapping, which stays out of
// scope for the spike (see `src/app/input/mod.rs`'s real `key_map` module for
// what that actually requires).
actions!(gpui_shell_spike, [SplitDemo, Backspace]);

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
/// called anywhere in this module. Wiring an actual gpui-side repaint
/// trigger for PTY data is a separate concern for a later task (M0's
/// repaint-reliability check); a no-op wakeup is correct and sufficient
/// here.
pub fn spawn_terminal(
    cols: u16,
    rows: u16,
    cell_w: u16,
    cell_h: u16,
) -> anyhow::Result<Rc<Terminal>> {
    let config = Config::default();
    let wakeup: crate::term::Wakeup = Arc::new(|| {});
    let wakeup_gate = Arc::new(WakeupGate::new());
    let terminal = Terminal::new(
        &config,
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

/// The `Render` root view for the gpui-petruterm spike window.
pub struct GpuiShellRoot {
    pub terminals: Vec<Rc<Terminal>>,
    pub focus_handle: FocusHandle,
    /// Index into `terminals` that keyboard input is routed to. M0 spike
    /// minimal: no click-to-focus, no visual indicator — just enough to
    /// prove a split terminal is independently live. Defaults to the
    /// most-recently-spawned terminal.
    active_terminal: usize,
}

impl GpuiShellRoot {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let terminal = spawn_terminal(80, 24, 9, 18).expect("spawn initial terminal");

        // M0 repaint-reliability stand-in (per the migration spec): PTY output
        // arrives on a background reader thread, decoupled from any gpui
        // entity/state mutation gpui itself would notice — without this,
        // nothing repaints the terminal grid until an unrelated event (e.g.
        // the next keystroke) incidentally triggers one, reproducing the
        // exact `gotcha_lost_pty_echo_wakeup`/Zed-vi-mode class of bug this
        // spike exists to catch. A real event-driven wakeup (PTY output
        // directly notifying gpui, no polling) is real work for M1; this
        // ~30Hz poll is the spec-sanctioned, deliberately simple M0 fix.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break; // window/entity gone
                }
            }
        })
        .detach();

        Self {
            terminals: vec![terminal],
            focus_handle: cx.focus_handle(),
            active_terminal: 0,
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // M0 minimal: forward printable characters only. Full key-event mapping
        // (control chars, KKP, etc.) is out of scope for the spike.
        if let Some(ch) = &event.keystroke.key_char {
            if let Some(terminal) = self.terminals.get(self.active_terminal) {
                terminal.write_input(ch.as_bytes());
                cx.notify();
            }
        }
    }

    /// Demo action fired by the `ctrl-f %` chord (see `main`'s `cx.bind_keys`).
    /// Proves gpui's native keymap dispatch can reach real business logic:
    /// spawning a second live shell terminal side-by-side. Routes input to
    /// the newly spawned terminal so a human can verify it independently.
    fn on_split_demo(&mut self, _: &SplitDemo, _window: &mut Window, cx: &mut Context<Self>) {
        match spawn_terminal(80, 24, 9, 18) {
            Ok(terminal) => {
                self.terminals.push(terminal);
                self.active_terminal = self.terminals.len() - 1;
                cx.notify();
            }
            Err(e) => log::error!("gpui-shell spike: failed to spawn split terminal: {e:#}"),
        }
    }

    /// Fired by the `backspace` binding — sends DEL (0x7f), matching what the
    /// existing wgpu app's real key_map::translate_key sends for Backspace.
    fn on_backspace(&mut self, _: &Backspace, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(terminal) = self.terminals.get(self.active_terminal) {
            terminal.write_input(&[0x7f]);
            cx.notify();
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
            .on_action(cx.listener(Self::on_backspace))
            .flex()
            .size_full()
            .children(self.terminals.iter().map(|t| TerminalGridElement {
                terminal: t.clone(),
                cell_width: px(9.0),
                cell_height: px(18.0),
            }))
    }
}
