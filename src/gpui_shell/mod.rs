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
mod context_menu;
pub mod font_state;
mod info_overlay;
mod input;
mod key_map;
mod leader;
mod leader_dispatch;
mod mcp_overlay;
mod mouse;
mod palette;
mod palette_dispatch;
mod pane_view;
mod panes;
mod poll;
mod rasterize;
mod rename;
mod render;
mod render_callbacks;
mod render_sidebar;
mod search_bar;
mod separator;
pub mod sidebar;
mod sidebar_nav;
mod spawn_terminal;
mod standalone_keys;
pub mod status_bar;
pub mod tabs;
pub mod terminal_element;
pub mod text_input;
mod toast;
mod workspace;

pub use config_watch::spawn_config_watcher;
pub(crate) use spawn_terminal::spawn_terminal;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, AppContext, Context, FocusHandle, Focusable};

use crate::app::pty_schedule::WakeupGate;
use crate::config::Config;
use crate::llm::mcp::manager::McpManager;
use crate::llm::mcp::{config as mcp_config, trust};
use crate::llm::skills::SkillManager;
use crate::llm::steering::SteeringManager;
use crate::term::Terminal;
use crate::ui::palette::{Action, CommandPalette};
use crate::ui::search_bar::SearchBar;
use leader::LeaderAction;
use panes::PaneForest;

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
    /// The sidebar's own keyboard-focus identity, distinct from the root
    /// `focus_handle` -- lets `on_key_down` (`input.rs`) tell "the sidebar
    /// is open" (`sidebar.is_visible()`) apart from "the sidebar actually
    /// has keyboard focus right now" (`sidebar_focus_handle.is_focused
    /// (window)`), same distinction every other focusable surface in this
    /// codebase already needs (tab rename, workspace rename, the chat
    /// composer, the AI block).
    sidebar_focus_handle: FocusHandle,
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
    skill_manager: SkillManager,
    /// Steering-file content loaded the same way, at the same time.
    steering_manager: SteeringManager,
    /// MCP server connections, started once at startup (mirrors the wgpu
    /// build's own blocking `tokio_rt.block_on(mgr.start_all(&cfg))`,
    /// `src/app/ui/mod.rs` -- ported as-is rather than redesigned into an
    /// async poll-drain, since the reference itself blocks app construction
    /// here and an LLM-disabled session skips this entirely). `Arc` because
    /// tool-calling (out of scope for M3d, a future milestone) would need to
    /// share it with a spawned async task the same way the wgpu build's own
    /// `mcp_manager` field does.
    mcp_manager: Arc<McpManager>,
    /// The read-only content popup every sidebar row's activation opens
    /// (Task 4) -- see `info_overlay.rs`'s own doc comment for why it's
    /// modal and why that makes its `is_visible()` guard (`input.rs`)
    /// correct rather than a shortcut.
    info_overlay: info_overlay::InfoOverlay,
    /// The command palette's own state (query, filtered results, selected
    /// index, visibility) -- `crate::ui::palette::CommandPalette`, used
    /// directly rather than copied, the same relationship M3b/M3d
    /// established for `ChatPanel`/`SkillManager`/`McpManager`. `gpui_shell`
    /// always opens it via `open_with_items(..)` with its own filtered list
    /// (`palette_dispatch.rs`, Task 3) rather than `open()`'s unfiltered
    /// `all_actions` -- several of the wgpu build's own actions have no
    /// `gpui_shell` equivalent yet (see the M4 spec's §7 deferred list).
    palette: CommandPalette,
    /// The palette's query input -- a single persistent `TextInput` entity
    /// (unlike tab/workspace rename, which build a fresh one per edit; the
    /// palette opens/closes far more often, so its content is cleared and
    /// refocused on each open instead of rebuilding the widget). Its own
    /// `Submit`/`Cancel` actions are bound inside `TextInput`'s own
    /// `"TextInput"` key context (`text_input/mod.rs`'s
    /// `register_key_bindings`) and consumed by gpui's action-dispatch
    /// before they ever reach `on_key_down`'s bubble listener -- the
    /// `cx.subscribe` callback below (Step 4) is how this struct reacts to
    /// them instead.
    palette_query: gpui::Entity<text_input::TextInput>,
    /// Set when the palette confirms an action with no `Window` in hand
    /// (`cx.subscribe` callback) -- `render()`'s own top drains and
    /// dispatches it every frame. Same constraint M3b's chat `/q` close hit.
    pending_palette_action: Option<Action>,
    /// In-terminal text search (`Cmd+F`) -- `crate::ui::search_bar::
    /// SearchBar`, reused directly; drives real GPU-paint highlighting too.
    search_bar: SearchBar,
    /// The search query's own persistent `TextInput` entity -- same
    /// "cleared and refocused on each open, not rebuilt" shape as the
    /// palette's `palette_query` (M4a).
    search_query: gpui::Entity<text_input::TextInput>,
    /// The right-click context menu's own state -- see `context_menu.rs`'s
    /// own doc comment for why this isn't a reuse of `ContextMenu`.
    context_menu: context_menu::ContextMenu,
    /// Transient top-right notification -- see `toast.rs`'s own doc comment.
    toast: Option<(String, std::time::Instant)>,
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
        let palette = CommandPalette::new(&config);
        let palette_query =
            cx.new(|cx| text_input::TextInput::new(cx, &config.colors, "", "Type a command..."));
        // Set up once, for the widget's whole lifetime -- `palette_query` is
        // a persistent entity (Step 2's own doc comment), not rebuilt per
        // open like a rename editor, so this subscription only needs
        // creating once too.
        cx.subscribe(&palette_query, |this, _input, event, cx| {
            match event {
                text_input::TextInputEvent::Submit => {
                    if let Some(action) = this.palette.confirm() {
                        this.pending_palette_action = Some(action);
                    }
                }
                text_input::TextInputEvent::Cancel => {
                    this.palette.close();
                }
            }
            cx.notify();
        })
        .detach();

        let search_bar = SearchBar::default();
        let search_query =
            cx.new(|cx| text_input::TextInput::new(cx, &config.colors, "", "Search..."));
        cx.subscribe(&search_query, |this, _input, event, cx| {
            match event {
                text_input::TextInputEvent::Submit => this.search_bar.next_match(),
                text_input::TextInputEvent::Cancel => this.search_bar.close(),
            }
            cx.notify();
        })
        .detach();

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
            sidebar_focus_handle: cx.focus_handle(),
            chat,
            ai_block,
            skill_manager,
            steering_manager,
            mcp_manager,
            info_overlay: info_overlay::InfoOverlay::new(),
            palette,
            palette_query,
            pending_palette_action: None,
            search_bar,
            search_query,
            context_menu: context_menu::ContextMenu::default(),
            toast: None,
        }
    }
}

impl Focusable for GpuiShellRoot {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
