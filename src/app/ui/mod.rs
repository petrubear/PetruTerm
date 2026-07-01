use crate::app::mux::Mux;
use crate::app::renderer::RenderContext;
use crate::config::{self, Config};
use crate::llm::acp::terminal::AcpTerminalRequest;
use crate::llm::ai_block::AiBlock;
use crate::llm::chat_panel::{AiEvent, ChatPanel, ConfirmDisplay};
use crate::llm::mcp::config as mcp_config;
use crate::llm::mcp::manager::McpManager;
use crate::llm::shell_context::ShellContext;
use crate::llm::skills::SkillManager;
use crate::llm::steering::SteeringManager;
use crate::llm::tools::{execute_tool, AgentStepResult, AgentTool};
use crate::llm::LlmProvider;
use crate::ui::{CommandPalette, ContextMenu, Rect, SearchBar, SplitDir};
use crossbeam_channel::{Receiver, Sender};
use rust_i18n::t;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

mod git;
mod providers;

/// Spawn `AcpSession::connect` on the tokio runtime instead of blocking the
/// calling (UI) thread. `wakeup` nudges the winit event loop so `poll_acp_connect`
/// runs promptly once the connect finishes, even with no other pending events.
fn spawn_acp_connect(
    rt: &tokio::runtime::Runtime,
    agent_cfg: crate::config::schema::AcpAgentConfig,
    cwd: PathBuf,
    wakeup: EventLoopProxy<()>,
) -> tokio::sync::oneshot::Receiver<Result<crate::llm::acp::AcpSession, String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt.spawn(async move {
        let result = crate::llm::acp::AcpSession::connect(&agent_cfg, &cwd)
            .await
            .map_err(|e| format!("{e:#}"));
        let _ = tx.send(result);
        let _ = wakeup.send_event(());
    });
    rx
}

// Convert a raw LLM error string into an actionable user message (TD-038).
fn classify_llm_error(e: &str) -> String {
    let e_lower = e.to_ascii_lowercase();
    if e_lower.contains("401")
        || e_lower.contains("unauthorized")
        || e_lower.contains("invalid api key")
    {
        t!("ai.error.api_key").to_string()
    } else if e_lower.contains("429")
        || e_lower.contains("rate limit")
        || e_lower.contains("too many requests")
    {
        t!("ai.error.rate_limit").to_string()
    } else if e_lower.contains("connection")
        || e_lower.contains("connect")
        || e_lower.contains("network")
        || e_lower.contains("dns")
    {
        t!("ai.error.connection").to_string()
    } else if e_lower.contains("404")
        || e_lower.contains("model not found")
        || e_lower.contains("no such model")
    {
        t!("ai.error.model_not_found").to_string()
    } else if e_lower.contains("500")
        || e_lower.contains("502")
        || e_lower.contains("503")
        || e_lower.contains("server error")
    {
        t!("ai.error.server_error").to_string()
    } else if e_lower.contains("context")
        && (e_lower.contains("length") || e_lower.contains("limit") || e_lower.contains("exceed"))
    {
        t!("ai.error.context_exceeded").to_string()
    } else {
        e.to_string()
    }
}

// Inline text prompt shared by tab rename and workspace rename.
#[derive(Default)]
struct RenamePrompt {
    input: Option<String>,
}

impl RenamePrompt {
    fn start(&mut self, current: &str) {
        self.input = Some(current.to_string());
    }

    fn is_active(&self) -> bool {
        self.input.is_some()
    }

    fn as_deref(&self) -> Option<&str> {
        self.input.as_deref()
    }

    // Returns (consumed, confirmed_name). `consumed` is false when not active.
    fn handle_key(
        &mut self,
        key: &winit::keyboard::Key,
        cmd: bool,
        ctrl: bool,
    ) -> (bool, Option<String>) {
        if !self.is_active() {
            return (false, None);
        }
        use winit::keyboard::NamedKey;
        let confirm = match key {
            winit::keyboard::Key::Named(NamedKey::Escape) => {
                self.input = None;
                None
            }
            winit::keyboard::Key::Named(NamedKey::Enter) => {
                let raw = self.input.take().unwrap_or_default();
                let trimmed = raw.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            }
            winit::keyboard::Key::Named(NamedKey::Backspace) => {
                if let Some(s) = &mut self.input {
                    s.pop();
                }
                None
            }
            winit::keyboard::Key::Named(NamedKey::Space) => {
                if let Some(s) = &mut self.input {
                    s.push(' ');
                }
                None
            }
            winit::keyboard::Key::Character(s) if !cmd && !ctrl => {
                if let Some(input) = &mut self.input {
                    for ch in s.chars() {
                        input.push(ch);
                    }
                }
                None
            }
            _ => None,
        };
        (true, confirm)
    }
}

// Manages UI overlays: command palette, context menu, per-pane chat panels, and the inline AI block.
pub struct UiManager {
    pub palette: CommandPalette,
    pub context_menu: ContextMenu,

    // ── Chat panel (side panel, workspace-level) ──────────────────────────────
    pub(super) chat_panel: ChatPanel,
    /// Width used when creating new ChatPanels (kept in sync with config.llm.ui.width_cols).
    pub(super) panel_width_cols: u16,
    pub panel_focused: bool,
    /// True when Tab has been pressed and focus is on the file picker overlay.
    pub file_picker_focused: bool,

    // ── Inline AI block (Ctrl+Space, single-shot NL→command) ─────────────────
    pub ai_block: AiBlock,
    pub(super) block_tx: Sender<AiEvent>,
    pub(super) block_rx: Receiver<AiEvent>,

    pub llm_provider: Option<Arc<dyn LlmProvider>>,
    /// Error from the last `build_provider` call, shown to the user when llm_provider is None.
    pub(super) llm_init_error: Option<String>,
    pub tokio_rt: tokio::runtime::Runtime,
    /// TD-019: channel carries (panel_id, event) so tokens always reach the originating panel.
    pub ai_tx: Sender<(usize, AiEvent)>,
    pub ai_rx: Receiver<(usize, AiEvent)>,

    // ── Confirmation (write_file / run_command) ───────────────────────────────
    /// Oneshot sender to complete a pending confirmation. Consumed on y/n.
    pub(super) pending_confirm_tx: Option<tokio::sync::oneshot::Sender<bool>>,
    /// Undo stack: (path, original_content) pairs, newest first.
    pub undo_stack: VecDeque<(PathBuf, String)>,
    /// A confirmed run_command to forward to the active PTY. Consumed by app.rs.
    pub pending_pty_run: Option<String>,
    /// A confirmed inline agent action waiting to be dispatched (A-3). Consumed by app.rs.
    pub pending_agent_action: Option<crate::llm::agent_action::AgentAction>,

    // ── Status bar data ───────────────────────────────────────────────────────
    /// Cached git branch string for the current CWD. None = not yet fetched or not a repo.
    pub git_branch_cache: Option<String>,
    /// Instant of the last git branch fetch, for TTL-based refresh.
    pub(super) git_branch_fetched_at: Option<std::time::Instant>,
    /// Independent wall-clock timer for git poll (TD-PERF-19): we call poll_git_branch
    /// at most once per second, regardless of PTY/render activity.
    pub git_branch_last_poll: std::time::Instant,
    /// True while an async git fetch is in flight — prevents duplicate spawns (TD-PERF-19).
    pub(super) git_branch_in_flight: bool,
    /// Time when the current in-flight git fetch was spawned, for timeout detection (TD-PERF-19).
    pub(super) git_branch_spawn_time: Option<std::time::Instant>,
    /// Channel to receive async git branch results.
    pub(super) git_tx: crossbeam_channel::Sender<String>,
    pub git_rx: crossbeam_channel::Receiver<String>,
    /// CWD used for the last git branch fetch (to detect CWD changes).
    pub(super) git_branch_cwd: Option<PathBuf>,

    /// Handle for the in-flight LLM streaming task. Aborted on panel close or new query (TD-MEM-12).
    pub(super) streaming_handle: Option<tokio::task::JoinHandle<()>>,

    // ── Tab / workspace rename prompts ───────────────────────────────────────
    tab_rename: RenamePrompt,
    workspace_rename: RenamePrompt,

    // ── Text search (Cmd+F) ───────────────────────────────────────────────────
    pub search_bar: SearchBar,

    // ── Async file scan (TD-PERF-04) ─────────────────────────────────────────
    /// Receives scan results from the background file-picker scan thread.
    pub(super) file_scan_rx: Option<crossbeam_channel::Receiver<Vec<std::path::PathBuf>>>,

    // ── Async clipboard paste (TD-PERF-15) ───────────────────────────────────
    /// Receives clipboard text from the background paste thread.
    pub(super) pending_paste_rx: Option<crossbeam_channel::Receiver<String>>,

    // ── Async branch scan (TD-PERF-25) ───────────────────────────────────────
    /// Receives branch list from the background branch scan thread.
    pub(super) branch_scan_rx: Option<crossbeam_channel::Receiver<Vec<String>>>,
    /// CWD used for the in-flight branch scan (to build palette items on arrival).
    pub(super) branch_scan_cwd: Option<std::path::PathBuf>,

    // ── System prompt (loaded from ~/.config/petruterm/system/system_prompt.md) ─
    pub(super) system_prompt: String,

    // ── Skills (D-4) ─────────────────────────────────────────────────────────
    pub skill_manager: SkillManager,

    // ── Steering files ───────────────────────────────────────────────────────
    pub steering_manager: SteeringManager,

    // ── MCP (D-1/D-2/D-3) ────────────────────────────────────────────────────
    pub mcp_manager: std::sync::Arc<McpManager>,

    // ── ACP agent session (Phase 8) ───────────────────────────────────────────
    /// Active ACP session when `backend = Agent`. Dropped on `close_panel`.
    pub(super) acp_session: Option<crate::llm::acp::AcpSession>,
    /// Crossbeam sender — ACP tokio tasks forward `AcpTerminalRequest`s here.
    pub acp_terminal_tx: crossbeam_channel::Sender<AcpTerminalRequest>,
    /// Main-thread receiver for ACP terminal requests.
    pub acp_terminal_rx: crossbeam_channel::Receiver<AcpTerminalRequest>,
    /// Pending `terminal/wait_for_exit` responses: (pane_id, oneshot_sender).
    pub(super) pending_acp_wait_for_exit: Vec<(usize, tokio::sync::oneshot::Sender<i32>)>,
    /// In-flight background ACP connect, started by `new`/`rewire_backend`.
    /// Polled every frame via `poll_acp_connect` — connecting never blocks the UI thread.
    pub(super) acp_pending_connect:
        Option<tokio::sync::oneshot::Receiver<Result<crate::llm::acp::AcpSession, String>>>,
}

const AI_POLL_CAP: usize = 64;

#[derive(Default, Clone, Copy)]
pub struct AiPollResult {
    pub changed: bool,
    pub completed: bool,
    /// True when the channel still had events after the batch cap was hit.
    /// Callers should request another redraw to drain the remainder.
    pub more: bool,
}

impl UiManager {
    pub fn new(config: &Config, wakeup_proxy: EventLoopProxy<()>) -> Self {
        let (ai_tx, ai_rx) = crossbeam_channel::bounded(256);
        let (block_tx, block_rx) = crossbeam_channel::bounded(64);
        // Use a 2-thread runtime when LLM is enabled; single-threaded otherwise.
        // Saves 2 OS threads + scheduler overhead when AI is disabled (AUDIT-ENERGY-03).
        let tokio_rt = if config.llm.enabled {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("Failed to build tokio runtime")
        } else {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to build tokio runtime")
        };

        let (acp_terminal_tx, acp_terminal_rx) =
            crossbeam_channel::bounded::<AcpTerminalRequest>(32);

        let (llm_provider, llm_init_error, acp_pending_connect) = if config.llm.enabled {
            match config.llm.backend {
                crate::config::schema::LlmBackend::Provider => {
                    match crate::llm::build_provider(&config.llm) {
                        Ok(p) => (Some(p), None, None),
                        Err(e) => (None, Some(format!("{e:#}")), None),
                    }
                }
                crate::config::schema::LlmBackend::Agent => {
                    let pending = config.llm.agent.as_ref().map(|agent_cfg| {
                        let cwd = std::env::current_dir().unwrap_or_default();
                        spawn_acp_connect(
                            &tokio_rt,
                            agent_cfg.clone(),
                            cwd,
                            wakeup_proxy.clone(),
                        )
                    });
                    (None, None, pending)
                }
            }
        } else {
            (None, None, None)
        };

        let (git_tx_init, git_rx_init) = crossbeam_channel::bounded::<String>(1);

        let panel_width_cols = config.llm.ui.width_cols;
        let mut initial_panel = ChatPanel::new();
        initial_panel.width_cols = panel_width_cols;

        let mut palette = CommandPalette::new(config);
        palette.rebuild_snippets(&config.snippets);

        let system_prompt = crate::config::load_system_prompt();
        let mut skill_manager = SkillManager::new();
        let mut steering_manager = SteeringManager::new();
        if let Ok(cwd) = std::env::current_dir() {
            let trusted = crate::llm::mcp::trust::is_trusted(&cwd);
            if !trusted {
                let has_local = cwd.join(".petruterm/skills").exists()
                    || cwd.join(".petruterm/steering").exists();
                if has_local {
                    log::info!(
                        "Local skills/steering at {}/.petruterm/ not trusted — loading global only. \
                         Use 'Trust local MCP' in the palette to enable.",
                        cwd.display()
                    );
                }
            }
            skill_manager.load(&cwd, trusted);
            steering_manager.load(&cwd, trusted);
        }
        let skill_count = skill_manager.skills().len();

        // Skip MCP entirely when LLM is disabled — no AI panel, no tool calls (AUDIT-ENERGY-03).
        let mcp_manager = if config.llm.enabled {
            let mut mgr = McpManager::new();
            // Always load global MCP servers (installed by the user deliberately).
            if let Ok(mut cfg) = mcp_config::load_global() {
                // Load project-local MCP only if this cwd has been explicitly trusted.
                // This prevents a malicious repo's .petruterm/mcp.json from spawning
                // arbitrary processes when the directory is opened (AUDIT-SEC-02).
                if let Ok(cwd) = std::env::current_dir() {
                    let local_path = cwd.join(".petruterm/mcp.json");
                    if local_path.exists() {
                        if crate::llm::mcp::trust::is_trusted(&cwd) {
                            if let Ok(local) = mcp_config::load_local(&cwd) {
                                cfg.extend(local);
                            }
                        } else {
                            log::info!(
                                "Local MCP config found at {}/.petruterm/mcp.json but this \
                                 directory is not trusted — skipping. Use 'Trust local MCP' \
                                 in the command palette to enable.",
                                cwd.display()
                            );
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
            std::sync::Arc::new(mgr)
        } else {
            std::sync::Arc::new(McpManager::new())
        };
        let mcp_connected = mcp_manager.connected_count();

        initial_panel.mcp_connected = mcp_connected;
        initial_panel.skill_count = skill_count;

        Self {
            palette,
            context_menu: ContextMenu::new(),
            chat_panel: initial_panel,
            panel_width_cols,
            panel_focused: false,
            file_picker_focused: false,
            ai_block: AiBlock::new(),
            block_tx,
            block_rx,
            llm_provider,
            llm_init_error,
            tokio_rt,
            ai_tx,
            ai_rx,
            pending_confirm_tx: None,
            undo_stack: VecDeque::new(),
            pending_pty_run: None,
            pending_agent_action: None,
            git_branch_cache: None,
            git_branch_fetched_at: None,
            git_branch_last_poll: std::time::Instant::now(),
            git_branch_in_flight: false,
            git_branch_spawn_time: None,
            git_tx: git_tx_init,
            git_rx: git_rx_init,
            git_branch_cwd: None,
            streaming_handle: None,
            tab_rename: RenamePrompt::default(),
            workspace_rename: RenamePrompt::default(),
            search_bar: SearchBar::default(),
            file_scan_rx: None,
            pending_paste_rx: None,
            branch_scan_rx: None,
            branch_scan_cwd: None,
            system_prompt,
            skill_manager,
            steering_manager,
            mcp_manager,
            acp_session: None,
            acp_terminal_tx,
            acp_terminal_rx,
            pending_acp_wait_for_exit: Vec::new(),
            acp_pending_connect,
        }
    }

    /// Poll the in-flight background ACP connect started by `new`/`rewire_backend`.
    /// Non-blocking — call every frame while a connect is outstanding.
    pub fn poll_acp_connect(&mut self) {
        let Some(rx) = &mut self.acp_pending_connect else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(session)) => {
                self.acp_session = Some(session);
                self.llm_init_error = None;
                self.acp_pending_connect = None;
            }
            Ok(Err(e)) => {
                log::error!("ACP connect: {e}");
                self.llm_init_error = Some(e);
                self.acp_pending_connect = None;
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.llm_init_error = Some("ACP connect task ended unexpectedly".to_string());
                self.acp_pending_connect = None;
            }
        }
    }

    // ── Panel accessors ───────────────────────────────────────────────────────

    /// No-op — kept for call-site compatibility. Chat panel is now workspace-level.
    pub fn set_active_terminal(&mut self, _id: usize) {}

    pub fn panel(&self) -> &ChatPanel {
        &self.chat_panel
    }

    pub fn panel_mut(&mut self) -> &mut ChatPanel {
        &mut self.chat_panel
    }

    /// Drop streaming state for a closed terminal (TD-MEM-20). Safe to call with any id.
    pub fn remove_terminal_state(&mut self, _tid: usize) {
        if let Some(h) = self.streaming_handle.take() {
            h.abort();
        }
    }

    pub fn active_panel_id(&self) -> usize {
        0
    }

    pub fn is_panel_visible(&self) -> bool {
        self.chat_panel.is_visible()
    }

    pub fn is_block_visible(&self) -> bool {
        self.ai_block.is_visible()
    }

    // ── AI event polling ──────────────────────────────────────────────────────

    /// Poll streaming tokens for the chat panel. Returns true if content changed.
    /// TD-019: routes each event to the panel that originated the request (by panel_id),
    /// not the currently active panel — so tab-switching during streaming is safe.
    pub fn poll_ai_events(&mut self) -> AiPollResult {
        let mut result = AiPollResult::default();
        let mut count = 0;
        while count < AI_POLL_CAP {
            let Ok((_panel_id, event)) = self.ai_rx.try_recv() else {
                break;
            };
            result.changed = true;
            count += 1;
            let panel = &mut self.chat_panel;
            match event {
                AiEvent::Token(tok) => panel.append_token(&tok),
                AiEvent::Done => {
                    panel.mark_done();
                    result.completed = true;
                    // Auto-confirm inline actions when the user has opted in this session.
                    if panel.auto_confirm_actions
                        && matches!(
                            panel.state,
                            crate::llm::chat_panel::PanelState::ConfirmAction(_)
                        )
                    {
                        if let Some(action) = panel.resolve_action_yes() {
                            self.pending_agent_action = Some(action);
                        }
                    }
                }
                AiEvent::Error(e) => {
                    log::error!("LLM error: {e}");
                    panel.mark_error(classify_llm_error(&e));
                }
                AiEvent::Usage {
                    prompt_tokens,
                    completion_tokens,
                } => {
                    panel.last_prompt_tokens = prompt_tokens;
                    panel.last_completion_tokens = completion_tokens;
                }
                AiEvent::ToolStatus { tool, path, done } => {
                    panel.set_tool_status(&tool, &path, done);
                }
                AiEvent::ConfirmWrite { display, result_tx } => {
                    panel.mark_awaiting_confirm(display);
                    self.pending_confirm_tx = Some(result_tx);
                }
                AiEvent::ConfirmRun { cmd, result_tx } => {
                    panel.mark_awaiting_confirm(ConfirmDisplay::Run { cmd });
                    self.pending_confirm_tx = Some(result_tx);
                }
                AiEvent::UndoState { path, content } => {
                    const MAX_UNDO: usize = 10;
                    if self.undo_stack.len() >= MAX_UNDO {
                        self.undo_stack.pop_front();
                    }
                    self.undo_stack.push_back((path, content));
                }
            }
        }
        result.more = count == AI_POLL_CAP;
        result
    }

    /// Poll streaming tokens for the inline AI block. Returns true if content changed.
    pub fn poll_ai_block_events(&mut self) -> AiPollResult {
        let mut result = AiPollResult::default();
        let mut count = 0;
        while count < AI_POLL_CAP {
            let Ok(event) = self.block_rx.try_recv() else {
                break;
            };
            result.changed = true;
            count += 1;
            match event {
                AiEvent::Token(tok) => self.ai_block.append_token(&tok),
                AiEvent::Done => {
                    self.ai_block.mark_done();
                    result.completed = true;
                }
                AiEvent::Error(e) => {
                    log::error!("AI block error: {e}");
                    self.ai_block.mark_error(e);
                }
                AiEvent::ToolStatus { .. }
                | AiEvent::ConfirmWrite { .. }
                | AiEvent::ConfirmRun { .. }
                | AiEvent::UndoState { .. }
                | AiEvent::Usage { .. } => {} // AI block doesn't handle these
            }
        }
        result.more = count == AI_POLL_CAP;
        result
    }

    // ── Confirmation helpers ──────────────────────────────────────────────────

    /// User pressed [y] — apply the pending write/run.
    pub fn confirm_yes(&mut self) {
        if let Some(tx) = self.pending_confirm_tx.take() {
            // If it's a run_command, extract the cmd and queue it for PTY.
            if let Some(crate::llm::chat_panel::ConfirmDisplay::Run { cmd }) =
                self.panel().confirm_display.as_ref()
            {
                self.pending_pty_run = Some(cmd.clone());
            }
            let _ = tx.send(true);
            self.panel_mut().resolve_confirm();
        }
    }

    /// User pressed [n] — reject the pending write/run.
    pub fn confirm_no(&mut self) {
        if let Some(tx) = self.pending_confirm_tx.take() {
            let _ = tx.send(false);
            self.panel_mut().resolve_confirm();
        }
    }

    /// User confirmed an inline agent action (A-2). Extracts the action for A-3 to dispatch.
    pub fn confirm_action_yes(&mut self) {
        if let Some(action) = self.panel_mut().resolve_action_yes() {
            self.pending_agent_action = Some(action);
        }
    }

    /// User cancelled an inline agent action.
    pub fn confirm_action_no(&mut self) {
        self.panel_mut().resolve_action_no();
    }

    /// User pressed [a] — confirm this action and skip confirmation for future ones this session.
    pub fn confirm_action_always(&mut self) {
        self.panel_mut().auto_confirm_actions = true;
        self.confirm_action_yes();
    }

    /// Undo the last file write by restoring the saved original content.
    pub fn cmd_undo_last_write(&mut self) {
        if let Some((path, content)) = self.undo_stack.pop_back() {
            match std::fs::write(&path, &content) {
                Ok(_) => {
                    let msg = t!("ai.undo_restored", path = path.display().to_string()).to_string();
                    self.panel_mut()
                        .messages
                        .push(crate::llm::ChatMessage::assistant(msg));
                    self.panel_mut().dirty = true;
                }
                Err(e) => {
                    self.panel_mut().mark_error(format!("Undo failed: {e}"));
                }
            }
        }
    }

    // ── Async file scan (TD-PERF-04) ─────────────────────────────────────────

    /// Open the file picker and kick off a background scan of `cwd`.
    /// The picker shows immediately (empty list while scanning).
    pub fn open_file_picker_async(&mut self, cwd: std::path::PathBuf) {
        let panel = self.panel_mut();
        panel.file_picker_query.clear();
        panel.file_picker_cursor = 0;
        panel.file_picker_open = true;
        panel.file_picker_items.clear();
        panel.dirty = true;

        let (tx, rx) = crossbeam_channel::bounded(1);
        self.file_scan_rx = Some(rx);
        std::thread::spawn(move || {
            let mut items = crate::llm::chat_panel::scan_files(&cwd, 3);
            items.sort();
            let _ = tx.send(items);
        });
    }

    /// Drain scan results into the active panel. Returns true if items arrived.
    pub fn poll_file_scan(&mut self) -> bool {
        let Some(rx) = &self.file_scan_rx else {
            return false;
        };
        match rx.try_recv() {
            Ok(items) => {
                let panel = self.panel_mut();
                panel.file_picker_items = items;
                panel.dirty = true;
                self.file_scan_rx = None;
                true
            }
            Err(_) => false,
        }
    }

    // ── Async clipboard paste (TD-PERF-15) ───────────────────────────────────

    /// Start a background clipboard read. Result is retrieved via `poll_pending_paste`.
    pub fn request_paste_async(&mut self, wakeup: winit::event_loop::EventLoopProxy<()>) {
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.pending_paste_rx = Some(rx);
        std::thread::spawn(move || {
            let text = arboard::Clipboard::new()
                .ok()
                .and_then(|mut cb| cb.get_text().ok())
                .unwrap_or_default();
            let _ = tx.send(text);
            let _ = wakeup.send_event(());
        });
    }

    /// Returns clipboard text if a pending paste has completed.
    pub fn poll_pending_paste(&mut self) -> Option<String> {
        let rx = self.pending_paste_rx.as_ref()?;
        match rx.try_recv() {
            Ok(text) => {
                self.pending_paste_rx = None;
                Some(text)
            }
            Err(_) => None,
        }
    }

    // ── Tab / workspace rename prompts ───────────────────────────────────────

    pub fn start_tab_rename(&mut self, current_title: &str) {
        self.tab_rename.start(current_title);
    }

    pub fn start_workspace_rename(&mut self, current_name: &str) {
        self.workspace_rename.start(current_name);
    }

    pub fn is_renaming_tab(&self) -> bool {
        self.tab_rename.is_active()
    }

    pub fn tab_rename_text(&self) -> Option<&str> {
        self.tab_rename.as_deref()
    }

    /// Handle a key event while either rename prompt is active.
    /// Returns true if the event was consumed (caller should return immediately).
    pub fn handle_rename_key(
        &mut self,
        mux: &mut Mux,
        key: &winit::keyboard::Key,
        cmd: bool,
        ctrl: bool,
    ) -> bool {
        let (consumed, confirm) = self.tab_rename.handle_key(key, cmd, ctrl);
        if consumed {
            if let Some(name) = confirm {
                mux.tabs.rename_active(name);
            }
            return true;
        }
        let (consumed, confirm) = self.workspace_rename.handle_key(key, cmd, ctrl);
        if consumed {
            if let Some(name) = confirm {
                mux.cmd_rename_workspace(name);
            }
            return true;
        }
        false
    }

    // ── Chat panel operations ─────────────────────────────────────────────────

    /// Open the panel for `terminal_id`, auto-attaching `AGENTS.md` from `cwd`.
    pub fn open_panel_with_context(&mut self, terminal_id: usize, cwd: std::path::PathBuf) {
        self.set_active_terminal(terminal_id);
        if !self.panel().is_visible() {
            self.panel_mut().open();
        }
        self.panel_focused = true;
        self.file_picker_focused = false;
        self.panel_mut().init_default_files(&cwd);
    }

    pub fn close_panel(&mut self) {
        self.panel_mut().close();
        self.panel_focused = false;
        self.file_picker_focused = false;
        self.acp_session = None; // drop agent process
    }

    pub fn restart_chat_panel(&mut self) {
        if self.panel().file_picker_open {
            self.panel_mut().close_file_picker();
        }
        self.panel_mut().clear_messages();
    }

    pub fn copy_chat_panel_transcript(&self) {
        let Some(text) = self.panel().transcript_text() else {
            return;
        };
        std::thread::spawn(move || {
            let _ = arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text));
        });
    }

    /// Submit the current panel input. `cwd` is used for tool sandboxing.
    pub fn submit_ai_query(&mut self, wakeup_proxy: EventLoopProxy<()>, cwd: PathBuf) {
        // Canonicalize once — on macOS /var is a symlink to /private/var; without this
        // execute_tool's canon.starts_with(cwd) check always fails (TD-029).
        let cwd = cwd.canonicalize().unwrap_or(cwd);
        let panel_id = 0usize;
        let Some(user_content) = self.panel_mut().submit_input() else {
            return;
        };

        // ── ACP agent backend ─────────────────────────────────────────────────
        if self.acp_session.is_some() {
            let (ai_mpsc_tx, ai_mpsc_rx) = tokio::sync::mpsc::channel::<AiEvent>(256);
            let (term_mpsc_tx, term_mpsc_rx) = tokio::sync::mpsc::channel::<AcpTerminalRequest>(16);

            let send_result = self.acp_session.as_mut().unwrap().try_send_prompt(
                user_content,
                ai_mpsc_tx,
                term_mpsc_tx,
            );

            if let Err(e) = send_result {
                self.acp_session = None;
                self.panel_mut()
                    .mark_error(format!("ACP agent disconnected: {e:#}"));
                return;
            }

            let ai_tx_cb = self.ai_tx.clone();
            let term_tx_cb = self.acp_terminal_tx.clone();
            let wakeup = wakeup_proxy.clone();

            if let Some(h) = self.streaming_handle.take() {
                h.abort();
            }
            self.streaming_handle = Some(self.tokio_rt.spawn(async move {
                let mut ai_rx = ai_mpsc_rx;
                let mut term_rx = term_mpsc_rx;
                loop {
                    tokio::select! {
                        ev = ai_rx.recv() => match ev {
                            Some(event) => {
                                let _ = ai_tx_cb.send((panel_id, event));
                                let _ = wakeup.send_event(());
                            }
                            None => break,
                        },
                        req = term_rx.recv() => if let Some(r) = req {
                            let _ = term_tx_cb.send(r);
                        },
                    }
                }
            }));
            return;
        }

        // ── Provider backend ──────────────────────────────────────────────────
        let Some(provider) = self.llm_provider.clone() else {
            let msg = self
                .llm_init_error
                .clone()
                .unwrap_or_else(|| "LLM is disabled in config.".into());
            self.panel_mut().mark_error(msg);
            return;
        };

        self.panel_mut().context_window = provider.context_window();

        let mut system_text = self.system_prompt.clone();

        // Steering files: global/project Markdown rules always active.
        if let Some(block) = self.steering_manager.context_block() {
            system_text.push_str(&format!("\n\n{block}"));
        }

        // Skill injection (D-4): match by query, or keep the panel's active skill.
        let active_skill_name = self.panel().matched_skill.clone();
        let skill_match = {
            if let Some(skill) = self.skill_manager.match_query(&user_content) {
                let body = self.skill_manager.read_body(skill).ok();
                body.map(|b| (skill.name.clone(), b))
            } else if let Some(name) = &active_skill_name {
                // No new match — reuse the skill active in this conversation.
                let found = self
                    .skill_manager
                    .skills()
                    .iter()
                    .find(|s| &s.name == name)
                    .cloned();
                found.and_then(|s| {
                    self.skill_manager
                        .read_body(&s)
                        .ok()
                        .map(|b| (name.clone(), b))
                })
            } else {
                None
            }
        };
        if let Some((skill_name, skill_body)) = skill_match {
            system_text.push_str(&format!(
                "\n\nThe following expert skill has been activated. \
                 You MUST follow its instructions precisely. \
                 All files referenced in the instructions (templates, guides, scripts) \
                 are already included verbatim below — do NOT use file tools to read \
                 them from disk, their content is already here:\n\n{skill_body}"
            ));
            self.panel_mut().matched_skill = Some(skill_name);
        }

        if let Some(ctx) = ShellContext::load() {
            system_text.push_str(&format!(
                "\n\nShell context:\n{}",
                ctx.format_for_system_message()
            ));
        }

        // Inject attached file contents — capped at 512 KB/file and 1 MB total (TD-030).
        const MAX_FILE_BYTES: usize = 512 * 1024;
        const MAX_TOTAL_BYTES: usize = 1024 * 1024;
        let mut total_bytes = 0usize;
        let attached: Vec<_> = self.panel().attached_files.clone();
        for path in &attached {
            if total_bytes >= MAX_TOTAL_BYTES {
                break;
            }
            if let Ok(bytes) = std::fs::read(path) {
                let cap = bytes
                    .len()
                    .min(MAX_FILE_BYTES)
                    .min(MAX_TOTAL_BYTES - total_bytes);
                let content = String::from_utf8_lossy(&bytes[..cap]);
                let name = path.display();
                system_text.push_str(&format!("\n\n--- File: {name} ---\n{content}"));
                if cap < bytes.len() {
                    system_text.push_str("\n[... truncated — file exceeds size limit ...]");
                }
                total_bytes += cap;
            }
        }

        // Append inline action instructions so the LLM knows how to propose actions.
        system_text.push('\n');
        system_text.push('\n');
        system_text.push_str(crate::llm::agent_action::system_prompt_instructions());

        // Build initial API-format messages (Vec<Value> for tool-use compatibility).
        let mut api_msgs: Vec<serde_json::Value> =
            vec![serde_json::json!({"role": "system", "content": system_text})];
        for msg in &self.panel().messages {
            api_msgs.push(msg.to_api_value());
        }

        // MCP tools go first so the LLM encounters them before built-ins.
        // Built-ins whose functionality is covered by MCP are excluded to avoid
        // the LLM picking the more restricted built-in variant.
        let mcp_tool_names: Vec<String> = self
            .mcp_manager
            .all_tools()
            .into_iter()
            .map(|(_, t)| t.name.clone())
            .collect();
        let mut tool_specs = self.mcp_manager.all_tools_openai();
        tool_specs.extend(AgentTool::specs_excluding(&mcp_tool_names));
        let tx = self.ai_tx.clone();
        let mcp_manager = std::sync::Arc::clone(&self.mcp_manager);

        // Cancel any previous in-flight stream for this UI (TD-MEM-12).
        if let Some(h) = self.streaming_handle.take() {
            h.abort();
        }
        self.streaming_handle = Some(self.tokio_rt.spawn(async move {
            use futures_util::StreamExt;

            const MAX_TOOL_ROUNDS: usize = 5;

            for _round in 0..MAX_TOOL_ROUNDS {
                match provider.agent_step(&api_msgs, &tool_specs).await {
                    Err(e) => {
                        let _ = tx.send((panel_id, AiEvent::Error(e.to_string())));
                        let _ = wakeup_proxy.send_event(());
                        return;
                    }
                    Ok((AgentStepResult::Text(text), usage)) => {
                        // No tool calls — stream the final response normally by
                        // building a fresh stream from the completed messages.
                        // For simplicity, send the text as a single token.
                        if let Some(u) = usage {
                            let _ = tx.send((
                                panel_id,
                                AiEvent::Usage {
                                    prompt_tokens: u.prompt_tokens,
                                    completion_tokens: u.completion_tokens,
                                },
                            ));
                        }
                        let _ = tx.send((panel_id, AiEvent::Token(text)));
                        let _ = tx.send((panel_id, AiEvent::Done));
                        let _ = wakeup_proxy.send_event(());
                        return;
                    }
                    Ok((
                        AgentStepResult::ToolCalls {
                            assistant_msg,
                            calls,
                        },
                        _usage,
                    )) => {
                        // Add assistant's tool_calls message to history.
                        api_msgs.push(assistant_msg);

                        for call in &calls {
                            let path_str = call.path_arg().unwrap_or_default();

                            let result = if call.requires_confirmation() {
                                // ── Tools that need user confirmation ────────────────────
                                if call.name == "write_file" {
                                    match call.content_arg() {
                                        None => t!("ai.missing_content").to_string(),
                                        Some(new_content) => {
                                            let abs = cwd.join(&path_str);
                                            // Guard against path traversal: walk up to the
                                            // nearest existing ancestor, canonicalize it,
                                            // and verify it stays within cwd. This handles
                                            // both existing files and new files in new dirs.
                                            let is_safe = {
                                                let mut probe = abs.clone();
                                                loop {
                                                    match probe.canonicalize() {
                                                        Ok(c) => break c.starts_with(&cwd),
                                                        Err(_) => {
                                                            if !probe.pop() {
                                                                break false;
                                                            }
                                                        }
                                                    }
                                                }
                                            };
                                            if !is_safe {
                                                format!(
                                                    "Error: path '{}' is outside the working directory",
                                                    path_str
                                                )
                                            } else {
                                                let (confirm_tx, confirm_rx) =
                                                    tokio::sync::oneshot::channel::<bool>();
                                                // Compute diff in async task — keeps main thread unblocked (TD-PERF-31).
                                                let display =
                                                    crate::llm::chat_panel::ConfirmDisplay::for_write(
                                                        &abs,
                                                        &new_content,
                                                    );
                                                let _ = tx.send((
                                                    panel_id,
                                                    AiEvent::ConfirmWrite {
                                                        display,
                                                        result_tx: confirm_tx,
                                                    },
                                                ));
                                                let _ = wakeup_proxy.send_event(());
                                                match confirm_rx.await {
                                                    Ok(true) => {
                                                        let old = std::fs::read_to_string(&abs)
                                                            .unwrap_or_default();
                                                        let _ = tx.send((
                                                            panel_id,
                                                            AiEvent::UndoState {
                                                                path: abs.clone(),
                                                                content: old,
                                                            },
                                                        ));
                                                        match std::fs::write(&abs, &new_content) {
                                                            Ok(_) => {
                                                                t!("ai.file_written").to_string()
                                                            }
                                                            Err(e) => {
                                                                format!("Error writing file: {e}")
                                                            }
                                                        }
                                                    }
                                                    _ => t!("ai.write_rejected").to_string(),
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    // run_command
                                    let cmd = call.cmd_arg().unwrap_or_default();
                                    let (confirm_tx, confirm_rx) =
                                        tokio::sync::oneshot::channel::<bool>();
                                    let _ = tx.send((
                                        panel_id,
                                        AiEvent::ConfirmRun {
                                            cmd,
                                            result_tx: confirm_tx,
                                        },
                                    ));
                                    let _ = wakeup_proxy.send_event(());
                                    match confirm_rx.await {
                                        Ok(true) => t!("ai.command_sent").to_string(),
                                        _ => t!("ai.command_rejected").to_string(),
                                    }
                                }
                            } else if AgentTool::is_builtin(&call.name) {
                                // ── Built-in read-only tools — execute immediately ────────
                                let _ = tx.send((
                                    panel_id,
                                    AiEvent::ToolStatus {
                                        tool: call.name.clone(),
                                        path: path_str.clone(),
                                        done: false,
                                    },
                                ));
                                let _ = wakeup_proxy.send_event(());
                                let r = execute_tool(call, &cwd);
                                let _ = tx.send((
                                    panel_id,
                                    AiEvent::ToolStatus {
                                        tool: call.name.clone(),
                                        path: path_str.clone(),
                                        done: true,
                                    },
                                ));
                                let _ = wakeup_proxy.send_event(());
                                r
                            } else {
                                // ── MCP tool — route to the registered server ─────────────
                                let args =
                                    serde_json::from_str::<serde_json::Value>(&call.arguments)
                                        .unwrap_or(serde_json::json!({}));
                                let display_name = match mcp_manager.server_for_tool(&call.name) {
                                    Some(server) => format!("{}.{}", server, call.name),
                                    None => format!("mcp.{}", call.name),
                                };
                                let _ = tx.send((
                                    panel_id,
                                    AiEvent::ToolStatus {
                                        tool: display_name.clone(),
                                        path: String::new(),
                                        done: false,
                                    },
                                ));
                                let _ = wakeup_proxy.send_event(());
                                let r = match mcp_manager.call_tool(&call.name, args).await {
                                    Ok(text) => text,
                                    Err(e) => format!("MCP error: {e:#}"),
                                };
                                let _ = tx.send((
                                    panel_id,
                                    AiEvent::ToolStatus {
                                        tool: display_name,
                                        path: String::new(),
                                        done: true,
                                    },
                                ));
                                let _ = wakeup_proxy.send_event(());
                                r
                            };

                            // Add tool result to history.
                            api_msgs.push(serde_json::json!({
                                "role": "tool",
                                "tool_call_id": call.id,
                                "content": result,
                            }));
                        }
                        // Continue loop — LLM will get the tool results and respond.
                    }
                }
            }

            // Fallback: if tool rounds exhausted, do a final streaming call.
            // Drop tool-result messages (role:"tool") and assistant messages whose content
            // is empty (they only carried tool_calls) — both would be sent with the wrong
            // role or corrupt the conversation history (TD-033).
            match provider
                .stream(
                    api_msgs
                        .iter()
                        .filter_map(|v| {
                            let role = v.get("role")?.as_str()?;
                            if role == "tool" {
                                return None;
                            }
                            let content = v.get("content")?.as_str().unwrap_or("").to_string();
                            if role == "assistant" && content.is_empty() {
                                return None;
                            }
                            Some(crate::llm::ChatMessage {
                                role: match role {
                                    "user" => crate::llm::ChatRole::User,
                                    "assistant" => crate::llm::ChatRole::Assistant,
                                    _ => crate::llm::ChatRole::System,
                                },
                                content,
                            })
                        })
                        .collect(),
                )
                .await
            {
                Err(e) => {
                    let _ = tx.send((panel_id, AiEvent::Error(e.to_string())));
                }
                Ok(mut stream) => {
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(tok) => {
                                let _ = tx.send((panel_id, AiEvent::Token(tok)));
                            }
                            Err(e) => {
                                let _ = tx.send((panel_id, AiEvent::Error(e.to_string())));
                                break;
                            }
                        }
                        let _ = wakeup_proxy.send_event(());
                    }
                    let _ = tx.send((panel_id, AiEvent::Done));
                }
            }
            let _ = wakeup_proxy.send_event(());
        }));
    }

    pub fn explain_last_output(&mut self, mux: &Mux, wakeup_proxy: EventLoopProxy<()>) {
        let output = mux.last_terminal_lines(30);
        if output.is_empty() {
            return;
        }
        if !self.panel().is_visible() {
            self.panel_mut().open();
        }
        self.panel_focused = true;
        self.panel_mut().input = format!("Explain this terminal output:\n```\n{}\n```", output);
        let cwd = mux
            .active_cwd()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        self.submit_ai_query(wakeup_proxy, cwd);
    }

    pub fn fix_last_error(&mut self, mux: &Mux, wakeup_proxy: EventLoopProxy<()>) {
        let output = mux.last_terminal_lines(30);
        let ctx = ShellContext::load();
        let query = match &ctx {
            Some(c) if !c.last_command.is_empty() => format!(
                "The command `{}` failed (exit code {}). Output:\n```\n{}\n```\nHow do I fix this?",
                c.last_command, c.last_exit_code, output
            ),
            _ => format!(
                "This command failed. Output:\n```\n{}\n```\nHow do I fix this?",
                output
            ),
        };
        if !self.panel().is_visible() {
            self.panel_mut().open();
        }
        self.panel_focused = true;
        self.panel_mut().input = query;
        let cwd = mux
            .active_cwd()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        self.submit_ai_query(wakeup_proxy, cwd);
    }

    /// Execute the last AI-suggested command in the active terminal.
    pub fn chat_panel_run_command(&mut self, mux: &mut Mux) {
        if let Some(cmd) = self.panel().last_assistant_command() {
            let mut data = cmd.into_bytes();
            data.push(b'\r');
            if let Some(terminal) = mux.active_terminal() {
                terminal.write_input(&data);
            }
            self.close_panel();
        }
    }

    // ── Inline AI block operations ────────────────────────────────────────────

    /// Submit the current AI block query to the LLM (NL → shell command mode).
    pub fn submit_ai_block_query(&mut self, wakeup_proxy: EventLoopProxy<()>) {
        let query = self.ai_block.query.trim().to_string();
        if query.is_empty() {
            return;
        }

        let Some(provider) = self.llm_provider.clone() else {
            let msg = self
                .llm_init_error
                .clone()
                .unwrap_or_else(|| "LLM is disabled in config.".into());
            self.ai_block.mark_error(msg);
            return;
        };

        self.ai_block.set_loading();

        let mut system = "You are a shell command generator. The user describes what they want to do in natural language. Reply with ONLY the shell command to run — no explanation, no markdown, no code fences.".to_string();
        if let Some(ctx) = ShellContext::load() {
            system.push_str(&format!(
                "\n\nShell context:\n{}",
                ctx.format_for_system_message()
            ));
        }

        let messages = vec![
            crate::llm::ChatMessage::system(system),
            crate::llm::ChatMessage::user(&query),
        ];

        let tx = self.block_tx.clone();
        self.tokio_rt.spawn(async move {
            use futures_util::StreamExt;
            match provider.stream(messages).await {
                Err(e) => {
                    let _ = tx.send(AiEvent::Error(e.to_string()));
                }
                Ok(mut stream) => {
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(tok) => {
                                let _ = tx.send(AiEvent::Token(tok));
                            }
                            Err(e) => {
                                let _ = tx.send(AiEvent::Error(e.to_string()));
                                break;
                            }
                        }
                        let _ = wakeup_proxy.send_event(());
                    }
                    let _ = tx.send(AiEvent::Done);
                }
            }
            let _ = wakeup_proxy.send_event(());
        });
    }

    /// Run the command from the AI block response in the active terminal.
    pub fn run_ai_block_command(&mut self, mux: &mut Mux) {
        if let Some(cmd) = self.ai_block.command_to_run() {
            let mut data = cmd.into_bytes();
            data.push(b'\r');
            if let Some(terminal) = mux.active_terminal() {
                terminal.write_input(&data);
            }
            self.ai_block.close();
        }
    }

    // ── Palette action dispatch ───────────────────────────────────────────────

    pub fn handle_palette_action(
        &mut self,
        action: crate::ui::palette::Action,
        mux: &mut Mux,
        render_ctx: &mut RenderContext,
        config: &mut Config,
        window: Option<&Window>,
        wakeup_proxy: EventLoopProxy<()>,
    ) {
        use crate::ui::palette::Action;
        match action {
            Action::CommandPalette => {
                self.palette.open();
            }
            Action::ReloadConfig => {
                if let Ok((new_cfg, _lua)) = config::reload() {
                    *config = new_cfg;
                    render_ctx
                        .renderer
                        .update_bg_color(config.colors.background_wgpu());
                    self.palette.rebuild_keybinds(config);
                    self.palette.rebuild_snippets(&config.snippets);
                    self.rewire_backend(config, wakeup_proxy);
                }
            }
            Action::OpenConfigFile => {
                let _ = std::process::Command::new("open")
                    .arg(config::config_path())
                    .spawn();
            }
            Action::OpenConfigFolder => {
                let _ = std::process::Command::new("open")
                    .arg(config::config_dir())
                    .spawn();
            }
            Action::NewTab => {
                let (cols, rows) = mux.active_terminal_size();
                let (cell_w, cell_h) = (
                    render_ctx.shaper.cell_width as u16,
                    render_ctx.shaper.cell_height as u16,
                );
                let viewport = Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 800.0,
                    h: 600.0,
                };
                let cwd = mux.active_cwd().or_else(|| std::env::current_dir().ok());
                mux.cmd_new_tab(
                    config,
                    viewport,
                    cols as u16,
                    rows as u16,
                    cell_w,
                    cell_h,
                    wakeup_proxy,
                    cwd,
                );
            }
            Action::CloseTab => mux.cmd_close_tab(),
            Action::NextTab => mux.tabs.next_tab(),
            Action::PrevTab => mux.tabs.prev_tab(),
            Action::SwitchToTab(n) => {
                mux.tabs.switch_to_index(n.saturating_sub(1));
            }
            Action::NewWorkspace => {
                let (cols, rows) = mux.active_terminal_size();
                let (cell_w, cell_h) = (
                    render_ctx.shaper.cell_width as u16,
                    render_ctx.shaper.cell_height as u16,
                );
                let name = format!("workspace {}", mux.workspaces.len() + 1);
                mux.cmd_new_workspace(name);
                let viewport = Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 800.0,
                    h: 600.0,
                };
                let cwd = std::env::current_dir().ok();
                mux.cmd_new_tab(
                    config,
                    viewport,
                    cols as u16,
                    rows as u16,
                    cell_w,
                    cell_h,
                    wakeup_proxy,
                    cwd,
                );
            }
            Action::CloseWorkspace => {
                mux.cmd_close_workspace();
            }
            Action::RenameWorkspace => {
                let current = mux
                    .workspaces
                    .iter()
                    .find(|w| w.id == mux.active_workspace_id)
                    .map(|w| w.name.clone())
                    .unwrap_or_default();
                self.start_workspace_rename(&current);
            }
            Action::NextWorkspace => {
                mux.cmd_next_workspace();
            }
            Action::PrevWorkspace => {
                mux.cmd_prev_workspace();
            }
            Action::SaveWorkspace => {
                if let Err(e) = mux.save_workspace() {
                    log::error!("save_workspace: {e}");
                }
            }
            Action::OpenSavedWorkspaces => {
                let items: Vec<crate::ui::palette::PaletteAction> =
                    crate::app::mux::snapshot::list_saved_workspaces()
                        .into_iter()
                        .map(|info| crate::ui::palette::PaletteAction {
                            name: format!(
                                "{} ({} tabs) — {}",
                                info.name, info.tab_count, info.saved_at
                            ),
                            action: crate::ui::palette::Action::RestoreWorkspace(
                                info.path.to_string_lossy().into_owned(),
                            ),
                            keybind: None,
                        })
                        .collect();
                if items.is_empty() {
                    self.palette.open();
                } else {
                    self.palette.open_with_items(items);
                }
            }
            Action::RestoreWorkspace(path) => {
                match crate::app::mux::snapshot::load_workspace(&std::path::PathBuf::from(&path)) {
                    Ok(snap) => {
                        let (cols, rows) = mux.active_terminal_size();
                        let viewport = Rect {
                            x: 0.0,
                            y: 0.0,
                            w: 800.0,
                            h: 600.0,
                        };
                        let cell_w = render_ctx.shaper.cell_width as u16;
                        let cell_h = render_ctx.shaper.cell_height as u16;
                        mux.restore_workspace(
                            snap,
                            config,
                            viewport,
                            cols as u16,
                            rows as u16,
                            cell_w,
                            cell_h,
                            wakeup_proxy,
                        );
                    }
                    Err(e) => log::error!("restore_workspace: {e}"),
                }
            }
            Action::SplitHorizontal => {
                let (cols, rows) = mux.active_terminal_size();
                let (cell_w, cell_h) = (
                    render_ctx.shaper.cell_width as u16,
                    render_ctx.shaper.cell_height as u16,
                );
                let cwd = mux.active_cwd().or_else(|| std::env::current_dir().ok());
                mux.cmd_split(
                    config,
                    SplitDir::Horizontal,
                    cols as u16,
                    rows as u16,
                    cell_w,
                    cell_h,
                    wakeup_proxy,
                    cwd,
                );
            }
            Action::SplitVertical => {
                let (cols, rows) = mux.active_terminal_size();
                let (cell_w, cell_h) = (
                    render_ctx.shaper.cell_width as u16,
                    render_ctx.shaper.cell_height as u16,
                );
                let cwd = mux.active_cwd().or_else(|| std::env::current_dir().ok());
                mux.cmd_split(
                    config,
                    SplitDir::Vertical,
                    cols as u16,
                    rows as u16,
                    cell_w,
                    cell_h,
                    wakeup_proxy,
                    cwd,
                );
            }
            Action::ClosePane => mux.cmd_close_pane(),
            Action::ZoomPane => mux.cmd_toggle_zoom_pane(),
            Action::FocusPane(dir) => mux.cmd_focus_pane_dir(dir),
            Action::ToggleFullscreen => {
                if let Some(w) = window {
                    let is_fs = w.fullscreen().is_some();
                    w.set_fullscreen(if is_fs {
                        None
                    } else {
                        Some(winit::window::Fullscreen::Borderless(None))
                    });
                }
            }
            Action::Quit => {
                if let Some(w) = window {
                    let _ = w.request_inner_size(winit::dpi::PhysicalSize::new(0u32, 0u32));
                }
            }
            Action::ToggleAiPanel | Action::ToggleAiMode => {
                let terminal_id = mux.focused_terminal_id();
                if self.panel().is_visible() {
                    self.close_panel();
                } else {
                    let cwd = mux
                        .active_cwd()
                        .or_else(|| std::env::current_dir().ok())
                        .unwrap_or_default();
                    self.open_panel_with_context(terminal_id, cwd);
                }
            }
            Action::FocusAiPanel => {
                if self.panel().is_visible() {
                    // Panel abierto: alternar focus entre chat y terminal.
                    self.panel_focused = !self.panel_focused;
                    if !self.panel_focused {
                        self.file_picker_focused = false;
                    }
                } else {
                    // Panel cerrado: abrirlo y darle focus.
                    let terminal_id = mux.focused_terminal_id();
                    let cwd = mux
                        .active_cwd()
                        .or_else(|| std::env::current_dir().ok())
                        .unwrap_or_default();
                    self.open_panel_with_context(terminal_id, cwd);
                }
            }
            Action::EnableAiFeatures => {
                let terminal_id = mux.focused_terminal_id();
                let cwd = mux
                    .active_cwd()
                    .or_else(|| std::env::current_dir().ok())
                    .unwrap_or_default();
                self.open_panel_with_context(terminal_id, cwd);
            }
            Action::DisableAiFeatures => {
                self.close_panel();
            }
            Action::ExplainLastOutput => self.explain_last_output(mux, wakeup_proxy),
            Action::FixLastError => self.fix_last_error(mux, wakeup_proxy),
            Action::UndoLastWrite => self.cmd_undo_last_write(),
            Action::ClearAiContext => self.restart_chat_panel(),
            Action::TrustLocalMcp => {
                let cwd = std::env::current_dir().unwrap_or_default();
                match crate::llm::mcp::trust::trust(&cwd) {
                    Ok(()) => {
                        log::info!("Trusted local MCP config for {}", cwd.display());
                        self.reload_mcp(&cwd);
                    }
                    Err(e) => log::warn!("Failed to trust local MCP: {e:#}"),
                }
            }
            Action::ToggleStatusBar => {
                config.status_bar.enabled = !config.status_bar.enabled;
            }
            Action::RenameTab => {
                let current = mux
                    .tabs
                    .active_tab()
                    .map(|t| t.title.clone())
                    .unwrap_or_default();
                self.start_tab_rename(&current);
            }
            Action::GitCheckout(branch) => {
                let cwd = mux
                    .active_cwd()
                    .or_else(|| std::env::current_dir().ok())
                    .unwrap_or_default();
                self.git_checkout(&branch, &cwd);
            }
            Action::ExpandSnippet(body) => {
                if let Some(terminal) = mux.active_terminal() {
                    terminal.scroll_to_bottom();
                    terminal.write_input(body.as_bytes());
                }
            }
            Action::OpenThemePicker => {
                self.open_theme_picker();
            }
            Action::SwitchTheme(name) => {
                let path = crate::config::themes_dir().join(format!("{name}.lua"));
                match crate::config::lua::load_theme(&path) {
                    Ok(scheme) => {
                        log::info!(
                            "Switched theme to '{name}': bg={:?} fg={:?} ansi[1]={:?}",
                            scheme.background,
                            scheme.foreground,
                            scheme.ansi[1]
                        );
                        render_ctx
                            .renderer
                            .update_bg_color(scheme.background_wgpu());
                        config.colors = scheme;
                        // Hash-based row cache auto-invalidates on color change — no manual dirty needed.
                    }
                    Err(e) => log::error!("Failed to load theme '{name}': {e}"),
                }
            }
            Action::Noop => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_branch_in_flight_prevents_duplicate_spawn() {
        let config = crate::config::Config::default();

        // Create a minimal UiManager for testing.
        let (git_tx, git_rx) = crossbeam_channel::bounded::<String>(1);
        let (block_tx, block_rx) = crossbeam_channel::bounded::<AiEvent>(1);
        let (ai_tx, ai_rx) = crossbeam_channel::bounded::<(usize, AiEvent)>(1);

        let (acp_tx, acp_rx) = crossbeam_channel::bounded::<AcpTerminalRequest>(1);
        let ui = UiManager {
            palette: CommandPalette::new(&config),
            context_menu: ContextMenu::new(),
            chat_panel: ChatPanel::new(),
            panel_width_cols: 80,
            panel_focused: false,
            file_picker_focused: false,
            ai_block: AiBlock::new(),
            block_tx,
            block_rx,
            llm_provider: None,
            llm_init_error: None,
            tokio_rt: tokio::runtime::Runtime::new().unwrap(),
            ai_tx,
            ai_rx,
            pending_confirm_tx: None,
            undo_stack: VecDeque::new(),
            pending_pty_run: None,
            pending_agent_action: None,
            git_branch_cache: None,
            git_branch_fetched_at: None,
            git_branch_last_poll: std::time::Instant::now(),
            git_branch_in_flight: false,
            git_branch_spawn_time: None,
            git_tx,
            git_rx,
            git_branch_cwd: None,
            streaming_handle: None,
            tab_rename: RenamePrompt::default(),
            workspace_rename: RenamePrompt::default(),
            search_bar: SearchBar::default(),
            file_scan_rx: None,
            pending_paste_rx: None,
            branch_scan_rx: None,
            branch_scan_cwd: None,
            system_prompt: String::new(),
            skill_manager: SkillManager::new(),
            steering_manager: SteeringManager::new(),
            mcp_manager: std::sync::Arc::new(McpManager::new()),
            acp_session: None,
            acp_terminal_tx: acp_tx,
            acp_terminal_rx: acp_rx,
            pending_acp_wait_for_exit: Vec::new(),
            acp_pending_connect: None,
        };

        // Simulate an in-flight fetch by setting the flag and spawn time.
        let mut ui = UiManager {
            git_branch_in_flight: true,
            git_branch_spawn_time: Some(std::time::Instant::now()),
            ..ui
        };
        let test_cwd = std::path::PathBuf::from("/tmp/test");

        // Call poll_git_branch — it should NOT spawn a new fetch because in_flight is true.
        let updated = ui.poll_git_branch(Some(&test_cwd), false, std::time::Duration::from_secs(5));

        // We expect that in_flight remains true (no new spawn triggered).
        assert!(
            ui.git_branch_in_flight,
            "Flag should remain true, preventing duplicate spawn"
        );
        assert!(!updated, "No message received, so updated should be false");
    }

    #[test]
    fn test_git_branch_timeout_recovery() {
        let config = crate::config::Config::default();

        // Create a minimal UiManager for testing.
        let (git_tx, git_rx) = crossbeam_channel::bounded::<String>(1);
        let (block_tx, block_rx) = crossbeam_channel::bounded::<AiEvent>(1);
        let (ai_tx, ai_rx) = crossbeam_channel::bounded::<(usize, AiEvent)>(1);

        let (acp_tx2, acp_rx2) = crossbeam_channel::bounded::<AcpTerminalRequest>(1);
        let mut ui = UiManager {
            palette: CommandPalette::new(&config),
            context_menu: ContextMenu::new(),
            chat_panel: ChatPanel::new(),
            panel_width_cols: 80,
            panel_focused: false,
            file_picker_focused: false,
            ai_block: AiBlock::new(),
            block_tx,
            block_rx,
            llm_provider: None,
            llm_init_error: None,
            tokio_rt: tokio::runtime::Runtime::new().unwrap(),
            ai_tx,
            ai_rx,
            pending_confirm_tx: None,
            undo_stack: VecDeque::new(),
            pending_pty_run: None,
            pending_agent_action: None,
            git_branch_cache: Some("main".to_string()),
            git_branch_fetched_at: Some(std::time::Instant::now()),
            git_branch_last_poll: std::time::Instant::now(),
            git_branch_in_flight: true,
            // Simulate a fetch that was spawned >30s ago
            git_branch_spawn_time: Some(
                std::time::Instant::now() - std::time::Duration::from_secs(35),
            ),
            git_tx,
            git_rx,
            git_branch_cwd: Some(std::path::PathBuf::from("/tmp/test")),
            streaming_handle: None,
            tab_rename: RenamePrompt::default(),
            workspace_rename: RenamePrompt::default(),
            search_bar: SearchBar::default(),
            file_scan_rx: None,
            pending_paste_rx: None,
            branch_scan_rx: None,
            branch_scan_cwd: None,
            system_prompt: String::new(),
            skill_manager: SkillManager::new(),
            steering_manager: SteeringManager::new(),
            mcp_manager: std::sync::Arc::new(McpManager::new()),
            acp_session: None,
            acp_terminal_tx: acp_tx2,
            acp_terminal_rx: acp_rx2,
            pending_acp_wait_for_exit: Vec::new(),
            acp_pending_connect: None,
        };

        // Cache should still be "main" before the call.
        assert_eq!(ui.git_branch_cache, Some("main".to_string()));
        assert!(ui.git_branch_in_flight);

        // Call poll_git_branch — it should detect the timeout (>30s) and reset in_flight.
        let updated = ui.poll_git_branch(
            Some(&std::path::PathBuf::from("/tmp/test")),
            false,
            std::time::Duration::from_secs(5),
        );

        // After timeout recovery:
        // - in_flight should be false (timeout recovered)
        // - cache should still be "main" (not cleared)
        // - updated should be false (no message received, just timeout recovery)
        assert!(
            !ui.git_branch_in_flight,
            "in_flight should be reset after >30s timeout"
        );
        assert_eq!(
            ui.git_branch_cache,
            Some("main".to_string()),
            "cache should remain unchanged (stale result preserved)"
        );
        assert!(!updated, "No message received, so updated should be false");
    }
}
