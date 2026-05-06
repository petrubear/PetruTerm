use crate::app::mux::Mux;
use crate::app::renderer::RenderContext;
use crate::config::{self, Config};
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

// Manages UI overlays: command palette, context menu, per-pane chat panels, and the inline AI block.
pub struct UiManager {
    pub palette: CommandPalette,
    pub context_menu: ContextMenu,

    // ── Chat panel (side panel, workspace-level) ──────────────────────────────
    chat_panel: ChatPanel,
    /// Width used when creating new ChatPanels (kept in sync with config.llm.ui.width_cols).
    panel_width_cols: u16,
    pub panel_focused: bool,
    /// True when Tab has been pressed and focus is on the file picker overlay.
    pub file_picker_focused: bool,

    // ── Inline AI block (Ctrl+Space, single-shot NL→command) ─────────────────
    pub ai_block: AiBlock,
    block_tx: Sender<AiEvent>,
    block_rx: Receiver<AiEvent>,

    pub llm_provider: Option<Arc<dyn LlmProvider>>,
    /// Error from the last `build_provider` call, shown to the user when llm_provider is None.
    llm_init_error: Option<String>,
    pub tokio_rt: tokio::runtime::Runtime,
    /// TD-019: channel carries (panel_id, event) so tokens always reach the originating panel.
    pub ai_tx: Sender<(usize, AiEvent)>,
    pub ai_rx: Receiver<(usize, AiEvent)>,

    // ── Confirmation (write_file / run_command) ───────────────────────────────
    /// Oneshot sender to complete a pending confirmation. Consumed on y/n.
    pending_confirm_tx: Option<tokio::sync::oneshot::Sender<bool>>,
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
    git_branch_fetched_at: Option<std::time::Instant>,
    /// Independent wall-clock timer for git poll (TD-PERF-19): we call poll_git_branch
    /// at most once per second, regardless of PTY/render activity.
    pub git_branch_last_poll: std::time::Instant,
    /// True while an async git fetch is in flight — prevents duplicate spawns (TD-PERF-19).
    git_branch_in_flight: bool,
    /// Time when the current in-flight git fetch was spawned, for timeout detection (TD-PERF-19).
    git_branch_spawn_time: Option<std::time::Instant>,
    /// Channel to receive async git branch results.
    git_tx: crossbeam_channel::Sender<String>,
    pub git_rx: crossbeam_channel::Receiver<String>,
    /// CWD used for the last git branch fetch (to detect CWD changes).
    git_branch_cwd: Option<PathBuf>,

    /// Handle for the in-flight LLM streaming task. Aborted on panel close or new query (TD-MEM-12).
    streaming_handle: Option<tokio::task::JoinHandle<()>>,

    // ── Tab rename prompt ─────────────────────────────────────────────────────
    /// When `Some`, the user is typing a new name for the active tab.
    pub tab_rename_input: Option<String>,
    /// When `Some`, the user is typing a new name for the active workspace.
    pub workspace_rename_input: Option<String>,

    // ── Text search (Cmd+F) ───────────────────────────────────────────────────
    pub search_bar: SearchBar,

    // ── Async file scan (TD-PERF-04) ─────────────────────────────────────────
    /// Receives scan results from the background file-picker scan thread.
    file_scan_rx: Option<crossbeam_channel::Receiver<Vec<std::path::PathBuf>>>,

    // ── Async clipboard paste (TD-PERF-15) ───────────────────────────────────
    /// Receives clipboard text from the background paste thread.
    pending_paste_rx: Option<crossbeam_channel::Receiver<String>>,

    // ── Async branch scan (TD-PERF-25) ───────────────────────────────────────
    /// Receives branch list from the background branch scan thread.
    branch_scan_rx: Option<crossbeam_channel::Receiver<Vec<String>>>,
    /// CWD used for the in-flight branch scan (to build palette items on arrival).
    branch_scan_cwd: Option<std::path::PathBuf>,

    // ── System prompt (loaded from ~/.config/petruterm/system/system_prompt.md) ─
    system_prompt: String,

    // ── Skills (D-4) ─────────────────────────────────────────────────────────
    pub skill_manager: SkillManager,

    // ── Steering files ───────────────────────────────────────────────────────
    pub steering_manager: SteeringManager,

    // ── MCP (D-1/D-2/D-3) ────────────────────────────────────────────────────
    pub mcp_manager: std::sync::Arc<McpManager>,
}

#[derive(Default, Clone, Copy)]
pub struct AiPollResult {
    pub changed: bool,
    pub completed: bool,
}

impl UiManager {
    pub fn new(config: &Config) -> Self {
        let (ai_tx, ai_rx) = crossbeam_channel::bounded(256);
        let (block_tx, block_rx) = crossbeam_channel::bounded(64);
        let tokio_rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");

        let (llm_provider, llm_init_error) = if config.llm.enabled {
            match crate::llm::build_provider(&config.llm) {
                Ok(p) => (Some(p), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            }
        } else {
            (None, None)
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
            skill_manager.load(&cwd);
            steering_manager.load(&cwd);
        }
        let skill_count = skill_manager.skills().len();

        let mcp_manager = {
            let mut mgr = McpManager::new();
            if let Ok(cwd) = std::env::current_dir() {
                if let Ok(cfg) = mcp_config::load(&cwd) {
                    if !cfg.is_empty() {
                        let errors = tokio_rt.block_on(mgr.start_all(&cfg));
                        for (name, err) in &errors {
                            log::warn!("MCP server '{name}' failed to start: {err:#}");
                        }
                    }
                }
            }
            std::sync::Arc::new(mgr)
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
            tab_rename_input: None,
            workspace_rename_input: None,
            search_bar: SearchBar::default(),
            file_scan_rx: None,
            pending_paste_rx: None,
            branch_scan_rx: None,
            branch_scan_cwd: None,
            system_prompt,
            skill_manager,
            steering_manager,
            mcp_manager,
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
        while let Ok((_panel_id, event)) = self.ai_rx.try_recv() {
            result.changed = true;
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
        result
    }

    /// Poll streaming tokens for the inline AI block. Returns true if content changed.
    pub fn poll_ai_block_events(&mut self) -> AiPollResult {
        let mut result = AiPollResult::default();
        while let Ok(event) = self.block_rx.try_recv() {
            result.changed = true;
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

    /// Poll for async git branch results and refresh the cache if due.
    /// Returns true if the cache was updated (caller should redraw).
    pub fn poll_git_branch(
        &mut self,
        cwd: Option<&std::path::Path>,
        dirty_check: bool,
        ttl: std::time::Duration,
    ) -> bool {
        // Drain any result that arrived from a previous spawn.
        let mut updated = false;
        while let Ok(branch) = self.git_rx.try_recv() {
            log::debug!("Git branch fetch completed: '{}'", branch);
            self.git_branch_cache = Some(branch);
            self.git_branch_fetched_at = Some(std::time::Instant::now());
            self.git_branch_in_flight = false;
            self.git_branch_spawn_time = None;
            updated = true;
        }

        // TD-PERF-19: Timeout recovery — if fetch is stuck in-flight for >30s, reset flag
        // without clearing the cache (keep stale result visible).
        if self.git_branch_in_flight {
            if let Some(spawn_time) = self.git_branch_spawn_time {
                if spawn_time.elapsed() > std::time::Duration::from_secs(30) {
                    log::warn!(
                        "Git branch fetch stuck for >30s, resetting in-flight flag (cache remains stale)"
                    );
                    self.git_branch_in_flight = false;
                    self.git_branch_spawn_time = None;
                }
            }
        }

        // Decide whether to spawn a fresh fetch.
        let cwd_changed = cwd.map(|p| p.to_path_buf()) != self.git_branch_cwd;
        let ttl_expired = self
            .git_branch_fetched_at
            .map(|t| t.elapsed() > ttl)
            .unwrap_or(true);

        if (cwd_changed || ttl_expired) && !self.git_branch_in_flight {
            if let Some(cwd_path) = cwd {
                self.git_branch_cwd = Some(cwd_path.to_path_buf());
                self.git_branch_in_flight = true;
                self.git_branch_spawn_time = Some(std::time::Instant::now());
                let tx = self.git_tx.clone();
                let cwd_owned = cwd_path.to_path_buf();
                log::debug!("Spawning git branch fetch for CWD: {:?}", cwd_owned);
                self.tokio_rt.spawn(async move {
                    let branch = fetch_git_branch(&cwd_owned, dirty_check).await;
                    let _ = tx.send(branch);
                });
            }
        }

        updated
    }

    /// Open the command palette in theme-picker mode.
    /// Lists .lua files in ~/.config/petruterm/themes/ and pre-populates the palette.
    pub fn open_theme_picker(&mut self) {
        use crate::ui::palette::{Action, PaletteAction};
        let themes = crate::config::list_themes();
        if themes.is_empty() {
            log::warn!(
                "No themes found in {}",
                crate::config::themes_dir().display()
            );
            return;
        }
        let items: Vec<PaletteAction> = themes
            .into_iter()
            .map(|name| PaletteAction {
                name: format!("  {name}"),
                action: Action::SwitchTheme(name),
                keybind: None,
            })
            .collect();
        self.palette.open_with_items(items);
    }

    /// Open the command palette in branch-picker mode.
    /// The palette opens immediately with a placeholder; branches populate async (TD-PERF-25).
    pub fn open_branch_picker(&mut self, cwd: &std::path::Path) {
        use crate::ui::palette::{Action, PaletteAction};
        let placeholder = vec![PaletteAction {
            name: t!("ai.loading_branches").to_string(),
            action: Action::Noop,
            keybind: None,
        }];
        self.palette.open_with_items(placeholder);
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.branch_scan_rx = Some(rx);
        self.branch_scan_cwd = Some(cwd.to_path_buf());
        let cwd_owned = cwd.to_path_buf();
        std::thread::spawn(move || {
            let branches = list_git_branches_sync(&cwd_owned);
            let _ = tx.send(branches);
        });
    }

    /// Drain branch scan results and repopulate the palette. Returns true if updated.
    pub fn poll_branch_scan(&mut self) -> bool {
        let Some(rx) = &self.branch_scan_rx else {
            return false;
        };
        match rx.try_recv() {
            Ok(branches) => {
                self.branch_scan_rx = None;
                if branches.is_empty() {
                    self.palette.close();
                    self.branch_scan_cwd = None;
                    return true;
                }
                use crate::ui::palette::{Action, PaletteAction};
                let current = self
                    .git_branch_cache
                    .as_deref()
                    .unwrap_or("")
                    .trim_end_matches('*');
                let items: Vec<PaletteAction> = branches
                    .into_iter()
                    .map(|b| {
                        let label = if b == current {
                            format!("  {b}  ✓")
                        } else {
                            format!("  {b}")
                        };
                        PaletteAction {
                            name: label,
                            action: Action::GitCheckout(b),
                            keybind: None,
                        }
                    })
                    .collect();
                self.palette.open_with_items(items);
                self.branch_scan_cwd = None;
                true
            }
            Err(_) => false,
        }
    }

    /// Run `git checkout <branch>` in `cwd` and invalidate the branch cache.
    pub fn git_checkout(&mut self, branch: &str, cwd: &std::path::Path) {
        let status = std::process::Command::new("git")
            .args(["-C", &cwd.to_string_lossy(), "checkout", branch])
            .status();
        match status {
            Ok(s) if s.success() => {
                // Invalidate cache so the status bar refreshes immediately.
                self.git_branch_cache = None;
                self.git_branch_fetched_at = None;
            }
            Ok(s) => log::warn!("git checkout {branch} exited with {s}"),
            Err(e) => log::error!("git checkout {branch} failed: {e}"),
        }
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

    // ── Tab rename prompt ─────────────────────────────────────────────────────

    /// Start the inline rename prompt, pre-filling with the current tab title.
    pub fn start_tab_rename(&mut self, current_title: &str) {
        self.tab_rename_input = Some(current_title.to_string());
    }

    pub fn tab_rename_type(&mut self, ch: char) {
        if let Some(s) = &mut self.tab_rename_input {
            s.push(ch);
        }
    }

    pub fn tab_rename_backspace(&mut self) {
        if let Some(s) = &mut self.tab_rename_input {
            s.pop();
        }
    }

    /// Confirm the rename: applies to `mux` and clears the prompt.
    pub fn tab_rename_confirm(&mut self, mux: &mut Mux) {
        if let Some(input) = self.tab_rename_input.take() {
            let trimmed = input.trim().to_string();
            if !trimmed.is_empty() {
                mux.tabs.rename_active(trimmed);
            }
        }
    }

    pub fn tab_rename_cancel(&mut self) {
        self.tab_rename_input = None;
    }

    pub fn is_renaming_tab(&self) -> bool {
        self.tab_rename_input.is_some()
    }

    // ── Workspace rename prompt ───────────────────────────────────────────────

    pub fn start_workspace_rename(&mut self, current_name: &str) {
        self.workspace_rename_input = Some(current_name.to_string());
    }

    pub fn workspace_rename_type(&mut self, ch: char) {
        if let Some(s) = &mut self.workspace_rename_input {
            s.push(ch);
        }
    }

    pub fn workspace_rename_backspace(&mut self) {
        if let Some(s) = &mut self.workspace_rename_input {
            s.pop();
        }
    }

    pub fn workspace_rename_confirm(&mut self, mux: &mut Mux) {
        if let Some(input) = self.workspace_rename_input.take() {
            let trimmed = input.trim().to_string();
            if !trimmed.is_empty() {
                mux.cmd_rename_workspace(trimmed);
            }
        }
    }

    pub fn workspace_rename_cancel(&mut self) {
        self.workspace_rename_input = None;
    }

    pub fn is_renaming_workspace(&self) -> bool {
        self.workspace_rename_input.is_some()
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
                                            let (confirm_tx, confirm_rx) =
                                                tokio::sync::oneshot::channel::<bool>();
                                            let abs = cwd.join(&path_str);
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
                                                        Ok(_) => t!("ai.file_written").to_string(),
                                                        Err(e) => {
                                                            format!("Error writing file: {e}")
                                                        }
                                                    }
                                                }
                                                _ => t!("ai.write_rejected").to_string(),
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

    /// Reload MCP servers from disk config. Creates a fresh McpManager, starts all
    /// servers, and replaces the Arc. Called on hot-reload of mcp.json (D-5).
    pub fn reload_mcp(&mut self, cwd: &std::path::Path) {
        match crate::llm::mcp::config::load(cwd) {
            Ok(cfg) => {
                let mut mgr = McpManager::new();
                let errors = self.tokio_rt.block_on(mgr.start_all(&cfg));
                for (name, err) in &errors {
                    log::warn!("MCP hot-reload: server '{name}' failed to start: {err:#}");
                }
                let connected = mgr.connected_count();
                self.mcp_manager = std::sync::Arc::new(mgr);
                self.chat_panel.mcp_connected = connected;
                log::info!("MCP hot-reloaded: {connected} server(s) connected.");
            }
            Err(e) => log::warn!("MCP hot-reload: failed to load config: {e:#}"),
        }
    }

    /// TD-020: Re-wire the LLM provider and panel width from a fresh config.
    /// Call this on every config reload (both hot-reload and palette-triggered).
    pub fn rewire_llm_provider(&mut self, config: &Config) {
        (self.llm_provider, self.llm_init_error) = if config.llm.enabled {
            match crate::llm::build_provider(&config.llm) {
                Ok(p) => (Some(p), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            }
        } else {
            (None, None)
        };
        self.panel_width_cols = config.llm.ui.width_cols;
        self.system_prompt = crate::config::load_system_prompt();
        if let Ok(cwd) = std::env::current_dir() {
            self.skill_manager.load(&cwd);
            self.steering_manager.load(&cwd);
        }
    }

    // ── Slash command dispatcher (D-4) ───────────────────────────────────────

    /// Handle a slash command entered in the AI panel input.
    /// Returns true if the command was recognized, false if unknown.
    /// The input field is cleared on entry regardless of outcome.
    pub fn handle_slash_command(&mut self, input: &str) -> bool {
        self.panel_mut().input.clear();
        self.panel_mut().dirty = true;

        let trimmed = input.trim_start_matches('/');
        let (cmd, args) = trimmed
            .split_once(' ')
            .map_or((trimmed, ""), |(c, a)| (c, a.trim()));

        match cmd {
            "q" | "quit" => {
                self.close_panel();
                true
            }
            "clear" | "reset" => {
                self.restart_chat_panel();
                true
            }
            "skills" => {
                let msg = {
                    let skills = self.skill_manager.skills();
                    if skills.is_empty() {
                        "No skills loaded. Place SKILL.md files in ~/.config/petruterm/skills/<name>/".to_string()
                    } else {
                        let filtered: Vec<String> = skills
                            .iter()
                            .filter(|s| {
                                args.is_empty()
                                    || s.name.contains(args)
                                    || s.description.contains(args)
                            })
                            .map(|s| format!("## {}\n{}", s.name, s.description))
                            .collect();
                        if filtered.is_empty() {
                            format!("No skills matching '{args}'")
                        } else {
                            format!("# Skills\n{}", filtered.join("\n"))
                        }
                    }
                };
                self.panel_mut()
                    .messages
                    .push(crate::llm::ChatMessage::assistant(msg));
                self.panel_mut().dirty = true;
                true
            }
            "mcp" => {
                let msg = if self.mcp_manager.connected_count() == 0 {
                    "No MCP servers connected.".to_string()
                } else {
                    let mut tools = self.mcp_manager.all_tools();
                    tools.sort_by(|(a, _), (b, _)| a.cmp(b));

                    // Group tool names by server, preserving sort order.
                    let mut servers: Vec<(String, Vec<String>)> = Vec::new();
                    for (server, tool) in &tools {
                        if let Some(entry) = servers.iter_mut().find(|(s, _)| s == server) {
                            entry.1.push(tool.name.clone());
                        } else {
                            servers.push((server.clone(), vec![tool.name.clone()]));
                        }
                    }

                    let lines: Vec<String> = servers
                        .iter()
                        .map(|(name, tool_names)| {
                            let n = tool_names.len();
                            format!(
                                "## {} ({} tool{})\n{}",
                                name,
                                n,
                                if n == 1 { "" } else { "s" },
                                tool_names.join(", ")
                            )
                        })
                        .collect();

                    let n = servers.len();
                    format!(
                        "# MCP ({} server{})\n{}",
                        n,
                        if n == 1 { "" } else { "s" },
                        lines.join("\n")
                    )
                };
                self.panel_mut()
                    .messages
                    .push(crate::llm::ChatMessage::assistant(msg));
                self.panel_mut().dirty = true;
                true
            }
            _ => {
                let msg = format!("Unknown command: /{cmd}. Try /clear, /skills, /mcp or /quit.");
                self.panel_mut()
                    .messages
                    .push(crate::llm::ChatMessage::assistant(msg));
                self.panel_mut().dirty = true;
                false
            }
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
                    self.rewire_llm_provider(config);
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

    /// Build markdown content for the info overlay when the user opens an MCP server.
    /// Shows a tool list with descriptions and JSON input schemas.
    pub fn mcp_overlay_content(&self, server_name: &str) -> String {
        let tools = self.mcp_manager.tools_for_server(server_name);
        let mut out = format!("# {server_name}\n\n");
        if tools.is_empty() {
            out.push_str("*No tools registered (server not connected or no tools).*\n");
            return out;
        }
        let n = tools.len();
        out.push_str(&format!("## Tools ({})\n\n", n));
        for tool in tools {
            out.push_str(&format!("### {}\n", tool.name));
            if !tool.description.is_empty() {
                out.push_str(&format!("{}\n\n", tool.description));
            }
            let schema = serde_json::to_string_pretty(&tool.input_schema).unwrap_or_default();
            if schema != "null" && !schema.is_empty() {
                out.push_str(&format!("```json\n{schema}\n```\n\n"));
            }
        }
        out
    }
}

/// Async helper: fetch the current git branch for `cwd`.
/// Returns the branch name (with dirty `*` suffix if uncommitted changes),
/// or an empty string if `cwd` is not a git repo.
async fn fetch_git_branch(cwd: &std::path::Path, dirty_check: bool) -> String {
    use tokio::process::Command;

    let branch = Command::new("git")
        .args(["-C", &cwd.to_string_lossy(), "branch", "--show-current"])
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    if branch.is_empty() {
        return String::new();
    }

    if !dirty_check {
        return branch;
    }

    let dirty = Command::new("git")
        .args(["-C", &cwd.to_string_lossy(), "status", "--porcelain"])
        .output()
        .await
        .ok()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    if dirty {
        format!("{branch}*")
    } else {
        branch
    }
}

/// Sync helper: list local git branches for `cwd` (runs in a background thread).
/// Returns branch names sorted alphabetically, empty vec if not a git repo.
fn list_git_branches_sync(cwd: &std::path::Path) -> Vec<String> {
    let out = std::process::Command::new("git")
        .args([
            "-C",
            &cwd.to_string_lossy(),
            "branch",
            "--format=%(refname:short)",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let mut branches: Vec<String> = out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    branches.sort();
    branches
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
            tab_rename_input: None,
            workspace_rename_input: None,
            search_bar: SearchBar::default(),
            file_scan_rx: None,
            pending_paste_rx: None,
            branch_scan_rx: None,
            branch_scan_cwd: None,
            system_prompt: String::new(),
            skill_manager: SkillManager::new(),
            steering_manager: SteeringManager::new(),
            mcp_manager: std::sync::Arc::new(McpManager::new()),
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
            tab_rename_input: None,
            workspace_rename_input: None,
            search_bar: SearchBar::default(),
            file_scan_rx: None,
            pending_paste_rx: None,
            branch_scan_rx: None,
            branch_scan_cwd: None,
            system_prompt: String::new(),
            skill_manager: SkillManager::new(),
            steering_manager: SteeringManager::new(),
            mcp_manager: std::sync::Arc::new(McpManager::new()),
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
