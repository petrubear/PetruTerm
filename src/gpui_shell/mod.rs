// gpui chrome migration (M0 foundation spike + M1a foundation fixes): live
// terminal rendering root.
//
// Owns one or more `term::Terminal` instances and renders them through
// `TerminalGridElement` (see `terminal_element.rs`). Keyboard input is
// forwarded from gpui's key-down events straight to the PTY via `key_map`'s
// full key-event mapping.

mod actions;
mod ai_block;
mod chat_panel;
mod config_watch;
pub mod font_state;
mod input;
mod key_map;
mod leader;
mod leader_dispatch;
mod mouse;
mod pane_view;
mod panes;
mod poll;
mod rasterize;
mod rename;
mod render;
pub mod sidebar;
pub mod status_bar;
pub mod tabs;
pub mod terminal_element;
pub mod text_input;
mod workspace;

pub use config_watch::spawn_config_watcher;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, Context, FocusHandle, Focusable};

use crate::app::pty_schedule::WakeupGate;
use crate::config::Config;
use crate::llm::mcp::manager::McpManager;
use crate::llm::mcp::{config as mcp_config, trust};
use crate::llm::skills::SkillManager;
use crate::llm::steering::SteeringManager;
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
    /// One workspace per named group of tabs+panes+zoom-state (M3c) --
    /// mirrors `Mux`'s workspace layer (`src/app/mux/mod.rs`) conceptually;
    /// see `workspace.rs`'s own doc comment for why the on-disk shape
    /// differs. `terminals`/`wakeup_gates` below stay flat, keyed by
    /// terminal id, since ids are already globally unique across every
    /// workspace and the poll loop wants one map to walk (design doc §3.6).
    workspaces: workspace::WorkspaceManager,
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
    /// The in-progress workspace rename, `Some` only while `Leader W ,` is
    /// being answered. Unlike `tab_rename`, a workspace id is globally
    /// unique (one shared counter on `WorkspaceManager`, not one per
    /// workspace) so no cross-workspace collision risk exists here -- see
    /// `switch_workspace_to_index`'s doc comment (`actions.rs`) for why
    /// `tab_rename` doesn't get the same guarantee.
    workspace_rename: Option<(usize, gpui::Entity<text_input::TextInput>)>,
    /// The workspace sidebar drawer -- one global drawer on the LEFT,
    /// mirroring `chat`'s drawer on the right (`render.rs`'s `middle_row`).
    sidebar: sidebar::WorkspaceSidebar,
    /// The AI chat panel -- one global drawer, not one per pane (see
    /// `chat_panel/mod.rs`'s doc comment on why the wgpu build's
    /// `panel_id`/`set_active_terminal` plumbing has no equivalent here).
    chat: chat_panel::ChatPanelView,
    /// The inline `Ctrl+Space` AI block -- a genuinely separate surface from
    /// `chat`, with its own state machine, composer, and streaming channel
    /// (see `ai_block.rs`'s doc comment on why the two must never share a
    /// channel).
    ai_block: ai_block::AiBlockView,
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
}

impl GpuiShellRoot {
    pub fn new(cx: &mut Context<Self>, config: Config) -> Self {
        let (terminal, gate) = spawn_terminal(80, 24, &config).expect("spawn initial terminal");
        let terminal_id = 0;

        // See `poll::spawn_poll_loop`'s doc comment for what this loop does
        // and why (M0/M1a repaint-reliability); its body was extracted there
        // to keep this file under the 400-line convention.
        poll::spawn_poll_loop(cx);

        let mut workspaces = workspace::WorkspaceManager::new();
        workspaces.new_workspace("ws1");
        workspaces.active_mut().tabs.new_tab("zsh");

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

impl Focusable for GpuiShellRoot {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
