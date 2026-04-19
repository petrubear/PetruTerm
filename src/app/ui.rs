use crate::app::mux::Mux;
use crate::app::renderer::RenderContext;
use crate::config::{self, Config};
use crate::llm::ai_block::AiBlock;
use crate::llm::chat_panel::{AiEvent, ChatPanel, ConfirmDisplay};
use crate::llm::shell_context::ShellContext;
use crate::llm::tools::{execute_tool, AgentStepResult, AgentTool};
use crate::llm::LlmProvider;
use crate::ui::{CommandPalette, ContextMenu, Rect, SearchBar, SplitDir};
use crossbeam_channel::{Receiver, Sender};
use std::collections::{HashMap, VecDeque};
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
        "API key invalid or missing. Check llm.api_key in ~/.config/petruterm/llm.lua".to_string()
    } else if e_lower.contains("429")
        || e_lower.contains("rate limit")
        || e_lower.contains("too many requests")
    {
        "Rate limit reached. Wait a moment and try again, or switch to a different model."
            .to_string()
    } else if e_lower.contains("connection")
        || e_lower.contains("connect")
        || e_lower.contains("network")
        || e_lower.contains("dns")
    {
        "Cannot reach LLM provider. Check your internet connection or provider URL in llm.lua"
            .to_string()
    } else if e_lower.contains("404")
        || e_lower.contains("model not found")
        || e_lower.contains("no such model")
    {
        "Model not found. Check llm.model in ~/.config/petruterm/llm.lua".to_string()
    } else if e_lower.contains("500")
        || e_lower.contains("502")
        || e_lower.contains("503")
        || e_lower.contains("server error")
    {
        "LLM provider returned a server error. Try again in a moment.".to_string()
    } else if e_lower.contains("context")
        && (e_lower.contains("length") || e_lower.contains("limit") || e_lower.contains("exceed"))
    {
        "Context window exceeded. Detach some files or start a new conversation.".to_string()
    } else {
        e.to_string()
    }
}

// Manages UI overlays: command palette, context menu, per-pane chat panels, and the inline AI block.
pub struct UiManager {
    pub palette: CommandPalette,
    pub context_menu: ContextMenu,

    // ── Chat panel (side panel, per-pane history) ─────────────────────────────
    chat_panels: HashMap<usize, ChatPanel>,
    active_panel_id: usize,
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

    // ── Text search (Cmd+F) ───────────────────────────────────────────────────
    pub search_bar: SearchBar,

    // ── Async file scan (TD-PERF-04) ─────────────────────────────────────────
    /// Receives scan results from the background file-picker scan thread.
    file_scan_rx: Option<crossbeam_channel::Receiver<Vec<std::path::PathBuf>>>,

    // ── Async clipboard paste (TD-PERF-15) ───────────────────────────────────
    /// Receives clipboard text from the background paste thread.
    pending_paste_rx: Option<crossbeam_channel::Receiver<String>>,
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
        let mut chat_panels = HashMap::new();
        chat_panels.insert(0usize, initial_panel);

        let mut palette = CommandPalette::new(config);
        palette.rebuild_snippets(&config.snippets);

        Self {
            palette,
            context_menu: ContextMenu::new(),
            chat_panels,
            active_panel_id: 0,
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
            git_branch_cache: None,
            git_branch_fetched_at: None,
            git_branch_last_poll: std::time::Instant::now(),
            git_branch_in_flight: false,
            git_tx: git_tx_init,
            git_rx: git_rx_init,
            git_branch_cwd: None,
            streaming_handle: None,
            tab_rename_input: None,
            search_bar: SearchBar::default(),
            file_scan_rx: None,
            pending_paste_rx: None,
        }
    }

    // ── Panel accessors ───────────────────────────────────────────────────────

    /// Update which terminal's chat history is active. Must be called whenever
    /// the focused terminal changes (tab/pane switch).
    pub fn set_active_terminal(&mut self, id: usize) {
        if self.active_panel_id == id {
            return;
        }
        self.active_panel_id = id;
        let width = self.panel_width_cols;
        self.chat_panels.entry(id).or_insert_with(|| {
            let mut p = ChatPanel::new();
            p.width_cols = width;
            p
        });
    }

    pub fn panel(&self) -> &ChatPanel {
        self.chat_panels
            .get(&self.active_panel_id)
            .expect("active panel not initialized — call set_active_terminal first")
    }

    pub fn panel_mut(&mut self) -> &mut ChatPanel {
        self.chat_panels
            .entry(self.active_panel_id)
            .or_insert_with(ChatPanel::new)
    }

    /// Drop all state for a closed terminal (TD-MEM-20). Safe to call with any id.
    pub fn remove_terminal_state(&mut self, tid: usize) {
        self.chat_panels.remove(&tid);
        if self.active_panel_id == tid {
            self.active_panel_id = 0;
            if let Some(h) = self.streaming_handle.take() {
                h.abort();
            }
        }
    }

    pub fn active_panel_id(&self) -> usize {
        self.active_panel_id
    }

    pub fn is_panel_visible(&self) -> bool {
        self.chat_panels
            .get(&self.active_panel_id)
            .map(|p| p.is_visible())
            .unwrap_or(false)
    }

    pub fn is_block_visible(&self) -> bool {
        self.ai_block.is_visible()
    }

    // ── AI event polling ──────────────────────────────────────────────────────

    /// Poll streaming tokens for the chat panel. Returns true if content changed.
    /// TD-019: routes each event to the panel that originated the request (by panel_id),
    /// not the currently active panel — so tab-switching during streaming is safe.
    pub fn poll_ai_events(&mut self) -> bool {
        let mut changed = false;
        while let Ok((panel_id, event)) = self.ai_rx.try_recv() {
            changed = true;
            // Skip events for panels that were removed after the task was spawned (TD-MEM-20).
            let Some(panel) = self.chat_panels.get_mut(&panel_id) else {
                continue;
            };
            match event {
                AiEvent::Token(tok) => panel.append_token(&tok),
                AiEvent::Done => panel.mark_done(),
                AiEvent::Error(e) => {
                    log::error!("LLM error: {e}");
                    panel.mark_error(classify_llm_error(&e));
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
        changed
    }

    /// Poll streaming tokens for the inline AI block. Returns true if content changed.
    pub fn poll_ai_block_events(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.block_rx.try_recv() {
            changed = true;
            match event {
                AiEvent::Token(tok) => self.ai_block.append_token(&tok),
                AiEvent::Done => self.ai_block.mark_done(),
                AiEvent::Error(e) => {
                    log::error!("AI block error: {e}");
                    self.ai_block.mark_error(e);
                }
                AiEvent::ToolStatus { .. }
                | AiEvent::ConfirmWrite { .. }
                | AiEvent::ConfirmRun { .. }
                | AiEvent::UndoState { .. } => {} // AI block doesn't handle these
            }
        }
        changed
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

    /// Poll for async git branch results and refresh the cache if due.
    /// Returns true if the cache was updated (caller should redraw).
    pub fn poll_git_branch(&mut self, cwd: Option<&std::path::Path>) -> bool {
        // Drain any result that arrived from a previous spawn.
        let mut updated = false;
        while let Ok(branch) = self.git_rx.try_recv() {
            self.git_branch_cache = Some(branch);
            self.git_branch_fetched_at = Some(std::time::Instant::now());
            self.git_branch_in_flight = false;
            updated = true;
        }

        // Decide whether to spawn a fresh fetch.
        let cwd_changed = cwd.map(|p| p.to_path_buf()) != self.git_branch_cwd;
        let ttl_expired = self
            .git_branch_fetched_at
            .map(|t| t.elapsed() > std::time::Duration::from_secs(5))
            .unwrap_or(true);

        if (cwd_changed || ttl_expired) && !self.git_branch_in_flight {
            if let Some(cwd_path) = cwd {
                self.git_branch_cwd = Some(cwd_path.to_path_buf());
                self.git_branch_in_flight = true;
                let tx = self.git_tx.clone();
                let cwd_owned = cwd_path.to_path_buf();
                self.tokio_rt.spawn(async move {
                    let branch = fetch_git_branch(&cwd_owned).await;
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
    /// Lists local git branches synchronously (fast) and pre-populates the palette.
    pub fn open_branch_picker(&mut self, cwd: &std::path::Path) {
        use crate::ui::palette::{Action, PaletteAction};
        let branches = self.tokio_rt.block_on(list_git_branches(cwd));
        if branches.is_empty() {
            return;
        }
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
                    let msg = format!("↩ Restored {}", path.display());
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

    /// Submit the current panel input. `cwd` is used for tool sandboxing.
    pub fn submit_ai_query(&mut self, wakeup_proxy: EventLoopProxy<()>, cwd: PathBuf) {
        // Canonicalize once — on macOS /var is a symlink to /private/var; without this
        // execute_tool's canon.starts_with(cwd) check always fails (TD-029).
        let cwd = cwd.canonicalize().unwrap_or(cwd);
        // TD-019: capture the originating panel id before any await so tokens are
        // routed back to the correct panel even if the user switches tabs mid-stream.
        let panel_id = self.active_panel_id;
        let Some(_user_content) = self.panel_mut().submit_input() else {
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

        let mut system_text = String::from(
            "You are a helpful AI assistant embedded in PetruTerm, a terminal emulator. \
             You can answer any question the user has — general knowledge, coding, writing, or anything else. \
             You also have tools to read files, list directories, write files, and run commands within the working directory. \
             Use those tools when the user asks about code or files in their project."
        );
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

        // Build initial API-format messages (Vec<Value> for tool-use compatibility).
        let mut api_msgs: Vec<serde_json::Value> =
            vec![serde_json::json!({"role": "system", "content": system_text})];
        for msg in &self.panel().messages {
            api_msgs.push(msg.to_api_value());
        }

        let tool_specs = AgentTool::all_specs();
        let tx = self.ai_tx.clone();

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
                    Ok(AgentStepResult::Text(text)) => {
                        // No tool calls — stream the final response normally by
                        // building a fresh stream from the completed messages.
                        // For simplicity, send the text as a single token.
                        let _ = tx.send((panel_id, AiEvent::Token(text)));
                        let _ = tx.send((panel_id, AiEvent::Done));
                        let _ = wakeup_proxy.send_event(());
                        return;
                    }
                    Ok(AgentStepResult::ToolCalls {
                        assistant_msg,
                        calls,
                    }) => {
                        // Add assistant's tool_calls message to history.
                        api_msgs.push(assistant_msg);

                        for call in &calls {
                            let path_str = call.path_arg().unwrap_or_default();

                            let result = if call.requires_confirmation() {
                                // ── Tools that need user confirmation ────────────────────
                                if call.name == "write_file" {
                                    match call.content_arg() {
                                        None => "Error: missing 'content' argument.".to_string(),
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
                                                        Ok(_) => {
                                                            "File written successfully.".to_string()
                                                        }
                                                        Err(e) => {
                                                            format!("Error writing file: {e}")
                                                        }
                                                    }
                                                }
                                                _ => "Write rejected by user.".to_string(),
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
                                        Ok(true) => "Command sent to terminal.".to_string(),
                                        _ => "Command rejected by user.".to_string(),
                                    }
                                }
                            } else {
                                // ── Read-only tools — execute immediately ─────────────────
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
            self.panel_mut().close();
            self.panel_focused = false;
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
                if let Ok(new_cfg) = config::reload() {
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
                    self.panel_mut().close();
                    self.panel_focused = false;
                    self.file_picker_focused = false;
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
                self.panel_mut().close();
                self.panel_focused = false;
                self.file_picker_focused = false;
            }
            Action::ExplainLastOutput => self.explain_last_output(mux, wakeup_proxy),
            Action::FixLastError => self.fix_last_error(mux, wakeup_proxy),
            Action::UndoLastWrite => self.cmd_undo_last_write(),
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
        }
    }
}

/// Async helper: fetch the current git branch for `cwd`.
/// Returns the branch name (with dirty `*` suffix if uncommitted changes),
/// or an empty string if `cwd` is not a git repo.
async fn fetch_git_branch(cwd: &std::path::Path) -> String {
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

    // Check for uncommitted changes.
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

/// Async helper: list local git branches for `cwd`.
/// Returns branch names sorted alphabetically, empty vec if not a git repo.
async fn list_git_branches(cwd: &std::path::Path) -> Vec<String> {
    use tokio::process::Command;
    let out = Command::new("git")
        .args([
            "-C",
            &cwd.to_string_lossy(),
            "branch",
            "--format=%(refname:short)",
        ])
        .output()
        .await
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
