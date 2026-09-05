// gpui chrome migration (M0 foundation spike + M1a foundation fixes): live
// terminal rendering root.
//
// Owns one or more `term::Terminal` instances and renders them through
// `TerminalGridElement` (see `terminal_element.rs`). Keyboard input is
// forwarded from gpui's key-down events straight to the PTY via `key_map`'s
// full key-event mapping.

pub mod font_state;
mod key_map;
mod leader;
mod mouse;
mod pane_view;
mod panes;
mod rasterize;
pub mod tabs;
pub mod terminal_element;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use std::time::Duration;

use gpui::{div, prelude::*, App, Context, FocusHandle, Focusable, KeyDownEvent, Render, Window};

use crate::app::pty_schedule::WakeupGate;
use crate::config::Config;
use crate::term::Terminal;
use leader::LeaderAction;
use pane_view::to_rgba;
use panes::{PaneForest, SplitDir};

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
    /// `new()`'s initial terminal (id 0) and every later spawn (splits, new
    /// tabs) draw ids from this one counter.
    next_terminal_id: usize,
    pub focus_handle: FocusHandle,
    config: Config,
    wakeup_gates: HashMap<usize, Arc<WakeupGate>>,
    cursor_blink_on: bool,
    cursor_last_blink: std::time::Instant,
    /// Last-painted pixel bounds of every leaf and split in the active tab,
    /// refreshed each frame by `pane_view`'s `on_children_prepainted` hooks.
    /// `Rc<RefCell<_>>` (rather than the plain field Task 1 left here)
    /// because those hooks are `'static` closures owned by the element tree:
    /// they have to write into the cache from inside a frame that `render()`
    /// has already returned from.
    rect_cache: Rc<RefCell<panes::RectCache>>,
    /// Render-time zoom filter: when `Some(terminal_id)`, that pane is drawn
    /// alone, filling the whole content area, and the tab's pane tree is not
    /// walked at all. Deliberately never written into `PaneTree`/
    /// `PaneForest` itself -- same design as the wgpu app's own zoom
    /// (`src/app/frame.rs`, which swaps in a single full-viewport `PaneInfo`
    /// instead of mutating the tree), so unzooming is just dropping this.
    zoomed_pane: Option<usize>,
    /// Leader-key ("Ctrl+F" by default) chorded-input state -- ported from
    /// `src/app/input/mod.rs`'s `leader_active`/`leader_deadline`. `true`
    /// between the leader keypress and the very next keystroke (which is
    /// then consumed as the chord's second key, whatever it is).
    leader_active: bool,
    /// Set when `leader_active` flips true; cleared (by the poll loop, or by
    /// the next keystroke consuming the chord) once it's no longer needed.
    /// Checked against `Instant::now()` each poll tick -- see `new()`'s
    /// `cx.spawn` loop -- so a leader press with no follow-up key expires on
    /// its own after `config.leader.timeout_ms`.
    leader_deadline: Option<std::time::Instant>,
    /// True from a `Leader Option+Arrow` resize until a keystroke arrives
    /// with Option no longer held (or a non-arrow key) -- lets repeated
    /// arrow presses keep resizing without re-pressing the leader each time,
    /// matching `src/app/input/mod.rs`'s own `resize_mode` field.
    resize_mode: bool,
    /// Single-key leader dispatch table, built once from `config.keys`'s
    /// `LEADER`-scoped bindings (`leader::build_leader_map`). Rebuilt
    /// wholesale on every config reload alongside the rest of `self.config`.
    leader_map: HashMap<String, LeaderAction>,
}

/// Maps a gpui named-key string to the resize direction it drives under
/// `Leader Option+Arrow` / resize-mode continuation. gpui's own arrow-key
/// strings ("left"/"right"/"up"/"down", see `key_map::translate_key`), not
/// winit's `NamedKey::Arrow*` variants -- different event model, see this
/// module's own doc comment on why gpui_shell can't import winit at all.
fn arrow_key_to_focus_dir(key: &str) -> Option<panes::FocusDir> {
    match key {
        "left" => Some(panes::FocusDir::Left),
        "right" => Some(panes::FocusDir::Right),
        "up" => Some(panes::FocusDir::Up),
        "down" => Some(panes::FocusDir::Down),
        _ => None,
    }
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
                                this.leader_map = leader::build_leader_map(
                                    &crate::config::keybind_view::leader_bindings_view(&new_config)
                                        .bindings,
                                );
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

                        // Detect shells that exited on their own (typing
                        // `exit`, Ctrl+D, a crash) -- nothing else in this
                        // module reads `Pty::rx`, so without this an exited
                        // shell's pane just sits there dead until the user
                        // notices and closes it by hand. Only `Exit` is
                        // acted on here; other PtyEvent variants (title
                        // changes, bell, OSC 52 clipboard) are drained too
                        // so the channel can't grow unbounded, but are
                        // otherwise a known, pre-existing gap in gpui_shell
                        // (nothing ever consumed them before this loop
                        // existed either) -- not this fix's concern.
                        let mut exited_terminals = Vec::new();
                        for (&id, terminal) in &this.terminals {
                            while let Ok(event) = terminal.pty.rx.try_recv() {
                                if matches!(event, crate::term::PtyEvent::Exit(_)) {
                                    exited_terminals.push(id);
                                }
                            }
                        }
                        for id in exited_terminals {
                            this.on_terminal_exited(id, cx);
                            should_notify = true;
                        }
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
                        // Leader-deadline expiry: `on_key_down` only ever
                        // SETS `leader_active`/`leader_deadline` (a key press
                        // always means the deadline hasn't fired yet, by
                        // definition -- this loop would have cleared
                        // `leader_active` first if it had), so expiry has to
                        // be checked from somewhere that runs independently
                        // of keystrokes. Piggybacks on this same 33ms tick
                        // rather than a dedicated timer.
                        if this.leader_active {
                            if let Some(deadline) = this.leader_deadline {
                                if std::time::Instant::now() >= deadline {
                                    this.leader_active = false;
                                    this.leader_deadline = None;
                                    should_notify = true; // status bar's leader indicator needs to clear
                                }
                            }
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

        let leader_map = leader::build_leader_map(
            &crate::config::keybind_view::leader_bindings_view(&config).bindings,
        );

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
            rect_cache: Rc::new(RefCell::new(panes::RectCache::default())),
            zoomed_pane: None,
            leader_active: false,
            leader_deadline: None,
            resize_mode: false,
            leader_map,
        }
    }

    /// Spawn a terminal for a new pane and split the focused one around it.
    /// The new pane's real size is whatever taffy gives it on the next frame
    /// (`pane_view::fit_terminal` resizes the PTY to match), so the spawn
    /// dimensions here are only a placeholder.
    fn split_focused(&mut self, dir: SplitDir) {
        let (terminal, gate) = match spawn_terminal(80, 24, &self.config) {
            Ok(pair) => pair,
            Err(e) => {
                log::error!("gpui-shell: failed to spawn terminal for split: {e:#}");
                return;
            }
        };
        let terminal_id = self.next_terminal_id;
        self.next_terminal_id += 1;
        self.terminals.insert(terminal_id, terminal);
        self.wakeup_gates.insert(terminal_id, gate);
        let active = self.tabs.active_index();
        self.tab_panes[active].split(dir, terminal_id);
        // Splitting while zoomed would otherwise create a pane the user
        // can't see (the zoomed one still fills the window) and move focus
        // to it -- their next keystroke would go somewhere invisible.
        self.zoomed_pane = None;
    }

    /// Close the focused pane and reap its terminal. A no-op when it's the
    /// tab's last pane (`PaneForest::close_focused` refuses that case --
    /// closing the last pane is closing the tab, which is Task 4's job).
    fn close_focused_pane(&mut self) {
        let active = self.tabs.active_index();
        let Some(closed) = self.tab_panes[active].close_focused() else {
            return;
        };
        // SIGHUP the shell before dropping our Rc<Terminal> (below): `Drop
        // for Pty` only closes the master fd (see its own doc comment) --
        // it does NOT signal the child or wait for the reader thread
        // first, unlike the full `Pty::shutdown()` sequence, which we
        // can't call here since `&Rc<Terminal>` never gives `&mut Pty`.
        // Without this, closing a pane whose shell is still alive and idle
        // hangs the whole app: closing the master fd while the reader
        // thread's blocking `read()` on that same fd is still outstanding
        // deadlocks on macOS/BSD (`Pty::request_exit`'s own doc comment),
        // and nothing was ever going to make that shell exit on its own.
        if let Some(terminal) = self.terminals.get(&closed) {
            terminal.pty.request_exit();
        }
        self.reap_pane(closed);
    }

    /// Auto-close a pane whose shell process has already exited on its own
    /// (typing `exit`, `Ctrl+D`, the shell crashing) -- detected via
    /// `PtyEvent::Exit` on `Pty::rx`, drained by the poll loop in `new()`.
    /// Mirrors the wgpu app's own `Mux::close_terminal` (src/app/mux/mod.rs)
    /// in full now that Task 4 gives us tab-closing machinery: multi-pane
    /// tabs just lose the one pane; a tab whose exited pane was its last
    /// one is closed entirely via `close_tab_at`, which quits the app
    /// outright if that was also the app's last tab (see its own doc
    /// comment) -- exactly `frame.rs`'s `if self.close_exited_terminals(..)
    /// { event_loop.exit(); }` behavior, just reached from gpui's
    /// `cx.quit()` instead of winit's `event_loop.exit()`.
    ///
    /// No `Pty::request_exit()` call for either branch, unlike
    /// `close_focused_pane`/`LeaderAction::CloseTab`: the child is already
    /// gone by the time this runs (that's how we heard about it), so the
    /// reader thread's blocking `read()` has already returned (EOF) rather
    /// than being outstanding -- none of the deadlock risk `request_exit`'s
    /// doc comment describes applies, and SIGHUP'ing an already-reaped pid
    /// risks hitting a since-reused pid for no benefit.
    fn on_terminal_exited(&mut self, terminal_id: usize, cx: &mut Context<Self>) {
        let Some(tab_idx) = self
            .tab_panes
            .iter()
            .position(|p| p.root.leaf_ids().contains(&terminal_id))
        else {
            return;
        };
        if self.tab_panes[tab_idx].close_specific(terminal_id) {
            self.reap_pane(terminal_id);
            return;
        }
        // close_specific only refuses when this was the tab's last pane --
        // close_tab_at's own leaf loop will then find exactly one leaf
        // (terminal_id itself), so signal_shells: false is always correct
        // here, never a guess.
        self.close_tab_at(tab_idx, false, cx);
    }

    /// Close the tab at `tab_idx` (not necessarily the active one -- a
    /// background tab's last pane can exit while a different tab is
    /// focused) and reap every leaf terminal it owned. Quits the whole app
    /// via `cx.quit()` instead when `tab_idx` is the app's only remaining
    /// tab: gpui_shell's `render()` indexes `self.tab_panes[active_index]`
    /// unconditionally, so leaving zero tabs open is not a state this app
    /// can render at all -- matching the wgpu app's own behavior for the
    /// equivalent situation (`frame.rs`'s `if self.close_exited_terminals(
    /// exited) { event_loop.exit(); }`, reached when `Mux::close_terminal`
    /// closes a tab and none remain), and matching ordinary terminal
    /// emulators generally (closing your only tab closes the window).
    ///
    /// `signal_shells`: `true` sends every leaf's shell a SIGHUP first (the
    /// user explicitly closing a tab whose shells may still be alive,
    /// `LeaderAction::CloseTab`'s own prior behavior); `false` skips it
    /// (`on_terminal_exited`, whose sole leaf is already known dead).
    /// Returns whether a tab was actually closed (false only if `tab_idx`
    /// didn't name a real tab -- quitting the app counts as "closed").
    fn close_tab_at(
        &mut self,
        tab_idx: usize,
        signal_shells: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.tabs.tab_count() <= 1 {
            cx.quit();
            return true;
        }
        let Some(tab_id) = self.tabs.tabs().get(tab_idx).map(|t| t.id) else {
            return false;
        };
        self.tabs.close_tab(tab_id);
        if tab_idx < self.tab_panes.len() {
            let forest = self.tab_panes.remove(tab_idx);
            for id in forest.root.leaf_ids() {
                if signal_shells {
                    if let Some(terminal) = self.terminals.get(&id) {
                        terminal.pty.request_exit();
                    }
                }
                self.reap_pane(id);
            }
        }
        true
    }

    /// Shared teardown for a terminal id that a `PaneForest` has just
    /// dropped from its tree (either call site above) -- keeps the two from
    /// drifting out of sync on which bookkeeping needs updating.
    fn reap_pane(&mut self, terminal_id: usize) {
        self.terminals.remove(&terminal_id);
        self.wakeup_gates.remove(&terminal_id);
        self.rect_cache.borrow_mut().leaves.remove(&terminal_id);
        if self.zoomed_pane == Some(terminal_id) {
            self.zoomed_pane = None;
        }
    }

    /// Zoom the focused pane to fill the window, or unzoom if it already is.
    /// Zooming a tab that only has one pane is meaningless, so it's ignored.
    fn toggle_zoom(&mut self) {
        let active = self.tabs.active_index();
        let focused = self.tab_panes[active].focused_terminal;
        self.zoomed_pane = match self.zoomed_pane {
            Some(id) if id == focused => None,
            _ if self.tab_panes[active].root.leaf_count() > 1 => Some(focused),
            _ => None,
        };
    }

    /// Execute one resolved leader-key action (`on_key_down`'s leader
    /// dispatch branch). See `leader::LeaderAction`'s doc comment for why
    /// the set stops at these ten variants.
    fn dispatch_leader_action(&mut self, action: LeaderAction, cx: &mut Context<Self>) {
        match action {
            LeaderAction::NewTab => {
                let (terminal, gate) = match spawn_terminal(80, 24, &self.config) {
                    Ok(pair) => pair,
                    Err(e) => {
                        log::error!("gpui-shell: failed to spawn terminal for new tab: {e:#}");
                        return;
                    }
                };
                let terminal_id = self.next_terminal_id;
                self.next_terminal_id += 1;
                self.terminals.insert(terminal_id, terminal);
                self.wakeup_gates.insert(terminal_id, gate);
                self.tabs.new_tab("zsh");
                self.tab_panes.push(PaneForest::new(terminal_id));
                // Same reasoning as `split_focused`: a zoomed pane from the
                // tab being left would otherwise linger, filling the window
                // even after the new tab (which has nothing zoomed) becomes
                // active.
                self.zoomed_pane = None;
            }
            LeaderAction::CloseTab => {
                // Mirrors `Mux::cmd_close_tab` (src/app/mux/mod.rs:792-807),
                // via the shared `close_tab_at` helper (also used by
                // `on_terminal_exited` for the "shell exited as a tab's
                // last pane" case) so the two close paths can't drift
                // apart. `signal_shells: true` since this tab's shells may
                // still be alive (the user is closing it explicitly, not
                // reacting to an exit already observed).
                self.close_tab_at(self.tabs.active_index(), true, cx);
            }
            LeaderAction::NextTab => self.tabs.next_tab(),
            LeaderAction::PrevTab => self.tabs.prev_tab(),
            // Documented no-op for this milestone (M2 Task 4 ruling): a real
            // rename needs a modal/inline text-input flow that doesn't exist
            // in gpui_shell yet -- that infra belongs to M4 (command-palette
            // era). `LeaderAction::RenameTab` and `Leader ,` stay wired up
            // for parity with the wgpu app's full action set; this is a
            // deliberate scope cut, not an oversight.
            LeaderAction::RenameTab => {
                log::info!(
                    "gpui-shell: tab rename not yet implemented (needs M4 modal-input infra)"
                );
                return;
            }
            LeaderAction::SplitHorizontal => self.split_focused(SplitDir::Horizontal),
            LeaderAction::SplitVertical => self.split_focused(SplitDir::Vertical),
            LeaderAction::ClosePane => self.close_focused_pane(),
            LeaderAction::ZoomPane => self.toggle_zoom(),
            LeaderAction::FocusPane(dir) => {
                let active = self.tabs.active_index();
                // Clone the Rc first, same reason as `on_drag` in render():
                // `focus_dir` needs `&mut self.tab_panes[..]` and
                // `&self.rect_cache`'s contents at once, which a single
                // `self.` borrow of both fields can't express.
                let rects = self.rect_cache.clone();
                let rects = rects.borrow();
                self.tab_panes[active].focus_dir(dir, &rects);
            }
        }
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.cursor_blink_on = true;
        self.cursor_last_blink = std::time::Instant::now();

        // ── Pane-resize mode continuation — Leader Option+Arrow started it
        // (below); while it's active, subsequent Option+Arrow presses keep
        // resizing without another leader press. Checked first, like
        // `src/app/input/mod.rs`'s own top-of-function resize_mode guard:
        // gpui has no `ModifiersChanged`-equivalent hook wired into
        // `on_key_down` here, so mode exit is inferred from the next
        // keystroke instead of Option's key-up -- the first key that isn't
        // an Option-held arrow clears it.
        if self.resize_mode {
            if event.keystroke.modifiers.alt {
                if let Some(dir) = arrow_key_to_focus_dir(&event.keystroke.key) {
                    let active = self.tabs.active_index();
                    self.tab_panes[active].adjust_ratio(dir, 0.05);
                    cx.notify();
                    return;
                }
            }
            self.resize_mode = false;
        }

        // ── Leader key activation ────────────────────────────────────────
        // Leader-deadline expiry piggybacks on the 33ms poll loop (`new()`'s
        // `cx.spawn` block) -- this branch only ever SETS leader_active/
        // leader_deadline, never expires them (a key press always means the
        // deadline hasn't fired yet, since the poll loop would have cleared
        // leader_active first if it had).
        if !self.leader_active
            && event.keystroke.modifiers.control
            && !event.keystroke.modifiers.shift
            && !event.keystroke.modifiers.platform
            && event.keystroke.key == self.config.leader.key
        {
            self.leader_active = true;
            self.leader_deadline = Some(
                std::time::Instant::now()
                    + std::time::Duration::from_millis(self.config.leader.timeout_ms),
            );
            cx.notify(); // leader-active indicator (future status bar) needs to see this
            return;
        }

        // ── Leader key dispatch ──────────────────────────────────────────
        if self.leader_active {
            self.leader_active = false;
            self.leader_deadline = None;

            // Leader + Option + Arrow → resize (TD-042 parity).
            if event.keystroke.modifiers.alt {
                if let Some(dir) = arrow_key_to_focus_dir(&event.keystroke.key) {
                    let active = self.tabs.active_index();
                    self.tab_panes[active].adjust_ratio(dir, 0.05);
                    self.resize_mode = true; // stay in resize mode for subsequent arrows
                    cx.notify();
                    return;
                }
            }

            // Leader + 1-9 → select tab by index (hardcoded, like Cmd+1-9).
            if let Ok(n) = event.keystroke.key.parse::<usize>() {
                if (1..=9).contains(&n) {
                    self.tabs.switch_to_index(n - 1);
                    cx.notify();
                    return;
                }
            }

            // Data-driven dispatch for this milestone's ten actions
            // (c/&/n/b/,/%/"/x/z/h/j/k/l, per config/default/keybinds.lua).
            if let Some(action) = self.leader_map.get(event.keystroke.key.as_str()).copied() {
                self.dispatch_leader_action(action, cx);
            }
            return;
        }

        // ── Cmd+1-9 — switch to tab N (standard macOS pattern) ───────────
        if event.keystroke.modifiers.platform {
            if let Ok(n) = event.keystroke.key.parse::<usize>() {
                if (1..=9).contains(&n) {
                    self.tabs.switch_to_index(n - 1);
                    cx.notify();
                    return;
                }
            }
        }

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
        let (cell_width, cell_height) = font_state::measured_cell_size();

        // A zoomed pane that no longer belongs to the active tab (tab switch,
        // pane closed) has to be dropped before it's used, mirroring the wgpu
        // app's own "zoomed pane no longer in active tab -- clear zoom" guard
        // in src/app/frame.rs.
        if let Some(id) = self.zoomed_pane {
            if !self.tab_panes[active_index].root.leaf_ids().contains(&id) {
                self.zoomed_pane = None;
            }
        }

        // Drop last frame's geometry before this frame's prepaint pass
        // repopulates it: a pane that just closed, or one belonging to a tab
        // that's no longer active, must not keep answering focus_dir's
        // nearest-neighbour search with a rect it no longer occupies.
        {
            let mut rects = self.rect_cache.borrow_mut();
            rects.leaves.clear();
            rects.separators.clear();
        }

        // Both callbacks below outlive `render()` (they're owned by the
        // element tree and run during event dispatch), so they hold a WEAK
        // handle -- exactly what `Context::listener` does internally, and for
        // the same reason: a strong `Entity<Self>` parked in a per-frame
        // closure would keep this view alive past window close.
        let view = cx.entity().downgrade();
        let focus_view = view.clone();
        let on_focus: pane_view::PaneFocusCallback = Rc::new(move |terminal_id, _window, cx| {
            focus_view
                .update(cx, |root, cx| {
                    let active = root.tabs.active_index();
                    if root.tab_panes[active].focused_terminal != terminal_id {
                        root.tab_panes[active].focused_terminal = terminal_id;
                        cx.notify();
                    }
                })
                .ok();
        });
        let drag_view = view;
        let on_drag: pane_view::SeparatorDragCallback =
            Rc::new(move |node_id, position, _window, cx| {
                drag_view
                    .update(cx, |root, cx| {
                        // Clone the Rc first: `drag_separator` needs `&mut
                        // self.tab_panes[..]` and `&self.rect_cache`'s
                        // contents at once, which a single `root.` borrow of
                        // both fields can't express.
                        let rects = root.rect_cache.clone();
                        let rects = rects.borrow();
                        let active = root.tabs.active_index();
                        root.tab_panes[active].drag_separator(
                            node_id,
                            f32::from(position.x),
                            f32::from(position.y),
                            &rects,
                        );
                        cx.notify();
                    })
                    .ok();
            });

        let pane_ctx = pane_view::PaneRenderCx {
            terminals: &self.terminals,
            focused: self.tab_panes[active_index].focused_terminal,
            colors: &self.config.colors,
            cell_width,
            cell_height,
            cursor_blink_on: self.cursor_blink_on,
            scrollback: self.config.scrollback_lines as usize,
            rects: self.rect_cache.clone(),
            on_focus,
            on_drag,
        };
        let panes = match self.zoomed_pane {
            Some(terminal_id) => pane_view::render_leaf(terminal_id, &pane_ctx),
            None => pane_view::render_pane_tree(&self.tab_panes[active_index].root, &pane_ctx),
        };

        let on_select_tab: tabs::TabSelectCallback =
            Rc::new(cx.listener(|this, idx: &usize, _window, cx| {
                if this.tabs.switch_to_index(*idx) {
                    cx.notify();
                }
            }));
        let tab_bar = tabs::render_tab_bar(&self.tabs, &self.config.colors, on_select_tab);

        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .size_full()
            .bg(to_rgba(self.config.colors.background))
            .child(tab_bar)
            // `min_h_0`: a flex item's automatic minimum size is its content
            // size, so without this the pane row refuses to shrink below the
            // terminal grid it contains and pushes the tab bar off-screen on
            // a small window.
            .child(div().flex().flex_1().min_h_0().child(panes))
    }
}
