// gpui chrome migration (M0 foundation spike + M1a foundation fixes): live
// terminal rendering root.
//
// Owns one or more `term::Terminal` instances and renders them through
// `TerminalGridElement` (see `terminal_element.rs`). Keyboard input is
// forwarded from gpui's key-down events straight to the PTY via `key_map`'s
// full key-event mapping.

pub mod font_state;
mod key_map;
mod mouse;
mod panes;
mod rasterize;
pub mod tabs;
pub mod terminal_element;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use std::time::Duration;

use gpui::{div, prelude::*, App, Context, FocusHandle, Focusable, KeyDownEvent, Render, Window};

use crate::app::pty_schedule::WakeupGate;
use crate::config::Config;
use crate::term::Terminal;
use mouse::OnFocusCallback;
use panes::PaneForest;
use terminal_element::TerminalGridElement;

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
    pub tabs: tabs::TabManager,
    /// Index-aligned with `tabs`'s tab list -- one PaneForest per tab,
    /// mirroring Mux.panes: Vec<PaneManager> in the wgpu app exactly.
    tab_panes: Vec<PaneForest>,
    /// terminal_id -> live Terminal handle. A tab's PaneForest only stores
    /// usize ids (matching src/ui/panes.rs's own design); this map is
    /// where the actual Rc<Terminal> lives, looked up by id wherever a
    /// leaf's real terminal is needed (paint, key routing, resize).
    terminals: HashMap<usize, Rc<Terminal>>,
    /// Not read until Task 3 starts spawning additional terminals (splits,
    /// new tabs); assigned now so `new()`'s initial terminal (id 0) and
    /// every later spawn draw ids from one counter.
    #[allow(dead_code)]
    next_terminal_id: usize,
    pub focus_handle: FocusHandle,
    config: Config,
    wakeup_gates: HashMap<usize, Arc<WakeupGate>>,
    cursor_blink_on: bool,
    cursor_last_blink: std::time::Instant,
    /// Not read until Task 3's paint pass populates and consumes it
    /// (focus_dir/adjust_ratio/drag_separator all need it) -- see
    /// panes::RectCache's own doc comment.
    #[allow(dead_code)]
    rect_cache: panes::RectCache,
}

impl GpuiShellRoot {
    pub fn new(cx: &mut Context<Self>, config: Config) -> Self {
        let (terminal, gate) = spawn_terminal(80, 24, &config).expect("spawn initial terminal");
        let terminal_id = 0;

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
                        let mut should_notify =
                            this.wakeup_gates.values().any(|g| g.take_pending());
                        // Blink at the same 530ms cadence the wgpu app uses
                        // (Input::update_cursor_blink). Piggybacks on this
                        // already-running 33ms poll loop instead of a new
                        // timer.
                        if this.cursor_last_blink.elapsed() >= std::time::Duration::from_millis(530)
                        {
                            this.cursor_blink_on = !this.cursor_blink_on;
                            this.cursor_last_blink = std::time::Instant::now();
                            should_notify = true;
                        }
                        if should_notify {
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

        let mut tabs = tabs::TabManager::new();
        tabs.new_tab("zsh");

        let mut terminals = HashMap::new();
        terminals.insert(terminal_id, terminal);
        let mut wakeup_gates = HashMap::new();
        wakeup_gates.insert(terminal_id, gate);

        Self {
            tabs,
            tab_panes: vec![PaneForest::new(terminal_id)],
            terminals,
            next_terminal_id: terminal_id + 1,
            focus_handle: cx.focus_handle(),
            config,
            wakeup_gates,
            cursor_blink_on: true,
            cursor_last_blink: std::time::Instant::now(),
            rect_cache: panes::RectCache::default(),
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor_blink_on = true;
        self.cursor_last_blink = std::time::Instant::now();
        let active_tid = self.tab_panes[self.tabs.active_index()].focused_terminal;
        let Some(terminal) = self.terminals.get(&active_tid) else {
            return;
        };
        // Any keystroke -- paste included -- snaps the view back to the
        // live edge, matching the wgpu app's own key handler
        // (src/app/input/mod.rs's scroll_to_bottom() call before every key
        // write). alacritty's grid deliberately pins a scrolled view even
        // as new output arrives, so without this a key press while
        // scrolled back leaves its own output landing off-screen.
        // `cx.notify()` here, not just below: a swallowed key (an unbound
        // Cmd-combo, e.g.) reaches neither this function's other `notify()`
        // calls, but scroll_to_bottom() already ran unconditionally above
        // -- without this, the view would jump to the bottom in Terminal
        // state but not on screen until the poll loop's own next incidental
        // repaint (up to 530ms later, the blink toggle).
        terminal.scroll_to_bottom();
        cx.notify();

        // Cmd+V paste. `key_map::translate_key` never sees this: gpui only
        // populates `key_char` when cmd is NOT held (see its own doc
        // comment), and there's no gpui keybinding action claiming Cmd+V
        // either, so it falls through as an unbound cmd-combo. Ported from
        // the wgpu app's own paste path (`frame.rs`'s `flush_pending_paste`)
        // minus its background-thread dance: that existed to keep arboard's
        // clipboard read off the main thread (TD-PERF-15), a cost gpui's own
        // `cx.read_from_clipboard()` doesn't have (a direct, already
        // in-process platform call).
        if event.keystroke.modifiers.platform && event.keystroke.key == "v" {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                if terminal.bracketed_paste_mode() {
                    let mut data = b"\x1b[200~".to_vec();
                    data.extend_from_slice(text.as_bytes());
                    data.extend_from_slice(b"\x1b[201~");
                    terminal.write_input(&data);
                } else {
                    terminal.write_input(text.as_bytes());
                }
                cx.notify();
            }
            return;
        }

        let mode = terminal.with_term(|term| *term.mode());
        if let Some(bytes) =
            key_map::translate_key(&event.keystroke, mode, self.config.keyboard.option_as_meta)
        {
            terminal.write_input(&bytes);
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

        let active_index = self.tabs.active_index();
        let active_tid = self.tab_panes[active_index].focused_terminal;
        let terminal = self
            .terminals
            .get(&active_tid)
            .expect("focused terminal id has a live Terminal")
            .clone();
        let (cell_width, cell_height) = font_state::measured_cell_size();
        // No click-to-focus target yet -- there's only ever one pane on
        // screen this task (Task 3 wires real multi-pane focus routing).
        let on_focus: OnFocusCallback = Rc::new(|_window, _cx| {});

        // Placeholder tab-bar row: just the labels, no click handling yet
        // (Task 3, once the real render-tree structure exists to attach it
        // to).
        let tab_bar =
            div()
                .flex()
                .flex_row()
                .w_full()
                .children(self.tabs.tabs().iter().enumerate().map(|(idx, tab)| {
                    div().px_2().py_1().child(tabs::tab_display_label(
                        &tab.title,
                        idx,
                        idx == active_index,
                        None,
                    ))
                }));

        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .size_full()
            .child(tab_bar)
            .child(div().flex().flex_1().child(TerminalGridElement {
                terminal,
                cell_width,
                cell_height,
                colors: self.config.colors.clone(),
                is_active: true,
                cursor_blink_on: self.cursor_blink_on,
                on_focus,
            }))
    }
}
