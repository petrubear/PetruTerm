// gpui chrome migration (M0 foundation spike + M1a foundation fixes): live
// terminal rendering root.
//
// Owns one or more `term::Terminal` instances and renders them through
// `TerminalGridElement` (see `terminal_element.rs`). Keyboard input is
// forwarded from gpui's key-down events straight to the PTY via `key_map`'s
// full key-event mapping.

mod actions;
mod chat_panel;
mod config_watch;
pub mod font_state;
mod input;
mod key_map;
mod leader;
mod mouse;
mod pane_view;
mod panes;
mod poll;
mod rasterize;
mod render;
pub mod status_bar;
pub mod tabs;
pub mod terminal_element;
pub mod text_input;

pub use config_watch::spawn_config_watcher;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, Context, FocusHandle, Focusable};

use crate::app::pty_schedule::WakeupGate;
use crate::config::Config;
use crate::term::Terminal;
use leader::LeaderAction;
use panes::PaneForest;

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
    /// Set when a two-key leader sub-prefix (currently only `a`, for AI
    /// actions) has been entered -- the wgpu build's own
    /// `src/app/input/mod.rs` shape (`leader_prefix: Option<char>`), mirrored
    /// here since `Leader a a` (M3b) is gpui_shell's first chord longer than
    /// one key. `leader_active`/`leader_deadline` stay re-armed while this is
    /// `Some` so the second key gets the same timeout as the first.
    leader_prefix: Option<char>,
    /// Single-key leader dispatch table, built once from `config.keys`'s
    /// `LEADER`-scoped bindings (`leader::build_leader_map`). Rebuilt
    /// wholesale on every config reload alongside the rest of `self.config`.
    leader_map: HashMap<String, LeaderAction>,
    /// Owned outright by `GpuiShellRoot` -- mirrors the wgpu app's own
    /// `tokio_rt` field placement on its `App`/`Mux` struct exactly (see
    /// this task's design ledger) rather than inventing a new pattern.
    /// `status_bar::poll_git_branch` spawns onto this each poll tick.
    tokio_rt: tokio::runtime::Runtime,
    /// Cached CWD of the active tab's focused terminal (status bar's CWD
    /// segment). Refreshed once per poll tick rather than every `render()`
    /// call -- see `new()`'s `cx.spawn` block for why a tick-based refresh
    /// was chosen over instrumenting every focus-changing call site.
    cached_cwd: Option<std::path::PathBuf>,
    /// Git-branch fetch/cache state for the status bar's GitBranch segment.
    git_branch: status_bar::GitBranchState,
    /// Exit-code cache for the status bar's ExitCode segment, mtime-gated
    /// against the active pane's shell-context file.
    exit_code: status_bar::ExitCodeState,
    /// The in-progress tab rename, `Some` only while `Leader ,` is being
    /// answered. Owning it here (rather than inside `TabManager`) keeps the
    /// tab data model free of gpui types, the same separation `StatusBar` and
    /// `PaneForest` already keep.
    ///
    /// Pinned to the target tab's **id** (`Tab.id`), not its index or "the
    /// active tab" -- the active tab can change out from under a rename
    /// (`Cmd+2`, `Leader n`, a tab click) while the editor is still open, and
    /// ids (unlike indices) stay stable across that. Commit resolves against
    /// this id via `TabManager::rename_tab`, and `render_tab_bar` places the
    /// editor on the cell whose `tab.id` matches it -- both independent of
    /// whichever tab happens to be active by the time either runs.
    tab_rename: Option<(usize, gpui::Entity<text_input::TextInput>)>,
    /// The AI chat panel -- one global drawer, not one per pane (see
    /// `chat_panel/mod.rs`'s doc comment on why the wgpu build's
    /// `panel_id`/`set_active_terminal` plumbing has no equivalent here).
    chat: chat_panel::ChatPanelView,
}

impl GpuiShellRoot {
    pub fn new(cx: &mut Context<Self>, config: Config) -> Self {
        let (terminal, gate) = spawn_terminal(80, 24, &config).expect("spawn initial terminal");
        let terminal_id = 0;

        // See `poll::spawn_poll_loop`'s doc comment for what this loop does
        // and why (M0/M1a repaint-reliability); its body was extracted there
        // to keep this file under the 400-line convention.
        poll::spawn_poll_loop(cx);

        let mut tabs = tabs::TabManager::new();
        tabs.new_tab("zsh");

        // Snapshot the initial pane's CWD before `terminal` moves into the
        // map below, so the status bar's CWD segment isn't empty until the
        // first 33ms poll tick runs.
        let initial_cwd = crate::term::process_cwd(terminal.child_pid);

        let mut terminals = HashMap::new();
        terminals.insert(terminal_id, terminal);
        let mut wakeup_gates = HashMap::new();
        wakeup_gates.insert(terminal_id, gate);

        let leader_map = leader::build_leader_map(
            &crate::config::keybind_view::leader_bindings_view(&config).bindings,
        );
        let chat = chat_panel::ChatPanelView::new(cx, &config);

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
            leader_prefix: None,
            leader_map,
            // Same construction pattern as the wgpu app's own `tokio_rt`
            // field on its `App`/`Mux` struct (`src/app/ui/mod.rs`).
            tokio_rt: tokio::runtime::Runtime::new().expect("Failed to build tokio runtime"),
            cached_cwd: initial_cwd,
            git_branch: status_bar::GitBranchState::default(),
            exit_code: status_bar::ExitCodeState::default(),
            tab_rename: None,
            chat,
        }
    }
}

impl Focusable for GpuiShellRoot {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
