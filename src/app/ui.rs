use std::collections::HashMap;
use std::sync::Arc;
use crossbeam_channel::{Receiver, Sender};
use crate::config::{self, Config};
use crate::llm::chat_panel::{AiEvent, ChatPanel};
use crate::llm::ai_block::AiBlock;
use crate::llm::LlmProvider;
use crate::llm::shell_context::ShellContext;
use crate::ui::{CommandPalette, SplitDir, Rect};
use winit::event_loop::EventLoopProxy;
use winit::window::Window;
use crate::app::mux::Mux;
use crate::app::renderer::RenderContext;

/// Manages UI overlays: command palette, per-pane chat panels, and the inline AI block.
pub struct UiManager {
    pub palette: CommandPalette,

    // ── Chat panel (side panel, per-pane history) ─────────────────────────────
    chat_panels: HashMap<usize, ChatPanel>,
    active_panel_id: usize,
    pub panel_focused: bool,
    /// True when Tab has been pressed and focus is on the file picker overlay.
    pub file_picker_focused: bool,

    // ── Inline AI block (Ctrl+Space, single-shot NL→command) ─────────────────
    pub ai_block: AiBlock,
    block_tx: Sender<AiEvent>,
    block_rx: Receiver<AiEvent>,

    pub llm_provider: Option<Arc<dyn LlmProvider>>,
    pub tokio_rt: tokio::runtime::Runtime,
    // Channel for chat panel streaming.
    pub ai_tx: Sender<AiEvent>,
    pub ai_rx: Receiver<AiEvent>,
}

impl UiManager {
    pub fn new(config: &Config) -> Self {
        let (ai_tx, ai_rx) = crossbeam_channel::unbounded();
        let (block_tx, block_rx) = crossbeam_channel::unbounded();
        let tokio_rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");

        let llm_provider = if config.llm.enabled {
            crate::llm::build_provider(&config.llm).ok()
        } else {
            None
        };

        let mut chat_panels = HashMap::new();
        chat_panels.insert(0usize, ChatPanel::new());

        Self {
            palette: CommandPalette::new(),
            chat_panels,
            active_panel_id: 0,
            panel_focused: false,
            file_picker_focused: false,
            ai_block: AiBlock::new(),
            block_tx,
            block_rx,
            llm_provider,
            tokio_rt,
            ai_tx,
            ai_rx,
        }
    }

    // ── Panel accessors ───────────────────────────────────────────────────────

    /// Update which terminal's chat history is active. Must be called whenever
    /// the focused terminal changes (tab/pane switch).
    pub fn set_active_terminal(&mut self, id: usize) {
        if self.active_panel_id == id { return; }
        self.active_panel_id = id;
        self.chat_panels.entry(id).or_insert_with(ChatPanel::new);
    }

    pub fn panel(&self) -> &ChatPanel {
        self.chat_panels.get(&self.active_panel_id)
            .expect("active panel not initialized — call set_active_terminal first")
    }

    pub fn panel_mut(&mut self) -> &mut ChatPanel {
        self.chat_panels.entry(self.active_panel_id).or_insert_with(ChatPanel::new)
    }

    pub fn is_panel_visible(&self) -> bool {
        self.chat_panels.get(&self.active_panel_id)
            .map(|p| p.is_visible())
            .unwrap_or(false)
    }

    pub fn is_block_visible(&self) -> bool {
        self.ai_block.is_visible()
    }

    // ── AI event polling ──────────────────────────────────────────────────────

    /// Poll streaming tokens for the chat panel. Returns true if content changed.
    pub fn poll_ai_events(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.ai_rx.try_recv() {
            changed = true;
            match event {
                AiEvent::Token(tok) => self.panel_mut().append_token(&tok),
                AiEvent::Done       => self.panel_mut().mark_done(),
                AiEvent::Error(e)   => {
                    log::error!("LLM error: {e}");
                    self.panel_mut().mark_error(e);
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
                AiEvent::Done       => self.ai_block.mark_done(),
                AiEvent::Error(e)   => {
                    log::error!("AI block error: {e}");
                    self.ai_block.mark_error(e);
                }
            }
        }
        changed
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

    pub fn submit_ai_query(&mut self, wakeup_proxy: EventLoopProxy<()>) {
        let Some(_user_content) = self.panel_mut().submit_input() else { return };
        let Some(provider) = self.llm_provider.clone() else {
            self.panel_mut().mark_error("LLM not configured".into());
            return;
        };

        let mut system_text = String::from("You are a helpful terminal assistant.");
        if let Some(ctx) = ShellContext::load() {
            system_text.push_str(&format!("\n\nShell context:\n{}", ctx.format_for_system_message()));
        }

        // Inject attached file contents into the system message.
        let attached: Vec<_> = self.panel().attached_files.clone();
        for path in &attached {
            if let Ok(content) = std::fs::read_to_string(path) {
                let name = path.display();
                system_text.push_str(&format!("\n\n--- File: {name} ---\n{content}"));
            }
        }

        let mut messages = vec![crate::llm::ChatMessage::system(system_text)];
        messages.extend(self.panel().messages.iter().cloned());

        let tx = self.ai_tx.clone();
        self.tokio_rt.spawn(async move {
            use futures_util::StreamExt;
            match provider.stream(messages).await {
                Err(e) => { let _ = tx.send(AiEvent::Error(e.to_string())); }
                Ok(mut stream) => {
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(tok) => { let _ = tx.send(AiEvent::Token(tok)); }
                            Err(e)  => { let _ = tx.send(AiEvent::Error(e.to_string())); break; }
                        }
                        let _ = wakeup_proxy.send_event(());
                    }
                    let _ = tx.send(AiEvent::Done);
                }
            }
            let _ = wakeup_proxy.send_event(());
        });
    }

    pub fn explain_last_output(&mut self, mux: &Mux, wakeup_proxy: EventLoopProxy<()>) {
        let output = mux.last_terminal_lines(30);
        if output.is_empty() { return; }
        if !self.panel().is_visible() { self.panel_mut().open(); }
        self.panel_focused = true;
        self.panel_mut().input = format!("Explain this terminal output:\n```\n{}\n```", output);
        self.submit_ai_query(wakeup_proxy);
    }

    pub fn fix_last_error(&mut self, mux: &Mux, wakeup_proxy: EventLoopProxy<()>) {
        let output = mux.last_terminal_lines(30);
        let ctx = ShellContext::load();
        let query = match &ctx {
            Some(c) if !c.last_command.is_empty() => format!(
                "The command `{}` failed (exit code {}). Output:\n```\n{}\n```\nHow do I fix this?",
                c.last_command, c.last_exit_code, output
            ),
            _ => format!("This command failed. Output:\n```\n{}\n```\nHow do I fix this?", output),
        };
        if !self.panel().is_visible() { self.panel_mut().open(); }
        self.panel_focused = true;
        self.panel_mut().input = query;
        self.submit_ai_query(wakeup_proxy);
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
        if query.is_empty() { return; }

        let Some(provider) = self.llm_provider.clone() else {
            self.ai_block.mark_error("LLM not configured".into());
            return;
        };

        self.ai_block.set_loading();

        let mut system = "You are a shell command generator. The user describes what they want to do in natural language. Reply with ONLY the shell command to run — no explanation, no markdown, no code fences.".to_string();
        if let Some(ctx) = ShellContext::load() {
            system.push_str(&format!("\n\nShell context:\n{}", ctx.format_for_system_message()));
        }

        let messages = vec![
            crate::llm::ChatMessage::system(system),
            crate::llm::ChatMessage::user(&query),
        ];

        let tx = self.block_tx.clone();
        self.tokio_rt.spawn(async move {
            use futures_util::StreamExt;
            match provider.stream(messages).await {
                Err(e) => { let _ = tx.send(AiEvent::Error(e.to_string())); }
                Ok(mut stream) => {
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(tok) => { let _ = tx.send(AiEvent::Token(tok)); }
                            Err(e)  => { let _ = tx.send(AiEvent::Error(e.to_string())); break; }
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
            Action::CommandPalette => { self.palette.open(); }
            Action::ReloadConfig => if let Ok(new_cfg) = config::reload() {
                *config = new_cfg;
                render_ctx.renderer.update_bg_color(config.colors.background_wgpu());
            },
            Action::OpenConfigFile => {
                let _ = std::process::Command::new("open").arg(config::config_path()).spawn();
            }
            Action::NewTab => {
                let (cols, rows) = mux.active_terminal_size();
                let (cell_w, cell_h) = (render_ctx.shaper.cell_width as u16, render_ctx.shaper.cell_height as u16);
                let viewport = Rect { x: 0.0, y: 0.0, w: 800.0, h: 600.0 };
                mux.cmd_new_tab(config, viewport, cols as u16, rows as u16, cell_w, cell_h, wakeup_proxy);
            }
            Action::CloseTab     => mux.cmd_close_tab(),
            Action::NextTab      => mux.tabs.next_tab(),
            Action::PrevTab      => mux.tabs.prev_tab(),
            Action::SwitchToTab(n) => { mux.tabs.switch_to_index(n.saturating_sub(1)); }
            Action::SplitHorizontal => {
                let (cols, rows) = mux.active_terminal_size();
                let (cell_w, cell_h) = (render_ctx.shaper.cell_width as u16, render_ctx.shaper.cell_height as u16);
                mux.cmd_split(config, SplitDir::Horizontal, cols as u16, rows as u16, cell_w, cell_h, wakeup_proxy);
            }
            Action::SplitVertical => {
                let (cols, rows) = mux.active_terminal_size();
                let (cell_w, cell_h) = (render_ctx.shaper.cell_width as u16, render_ctx.shaper.cell_height as u16);
                mux.cmd_split(config, SplitDir::Vertical, cols as u16, rows as u16, cell_w, cell_h, wakeup_proxy);
            }
            Action::ClosePane => mux.cmd_close_pane(),
            Action::ToggleFullscreen => if let Some(w) = window {
                let is_fs = w.fullscreen().is_some();
                w.set_fullscreen(if is_fs { None } else { Some(winit::window::Fullscreen::Borderless(None)) });
            },
            Action::Quit => {
                if let Some(w) = window {
                    let _ = w.request_inner_size(winit::dpi::PhysicalSize::new(0u32, 0u32));
                }
            }
            Action::ToggleAiPanel | Action::ToggleAiMode => {
                let terminal_id = mux.focused_terminal_id();
                if self.panel().is_visible() {
                    if self.panel_focused {
                        self.panel_mut().close();
                        self.panel_focused = false;
                        self.file_picker_focused = false;
                    } else {
                        self.panel_focused = true;
                    }
                } else {
                    let cwd = mux.active_cwd().or_else(|| std::env::current_dir().ok()).unwrap_or_default();
                    self.open_panel_with_context(terminal_id, cwd);
                }
            }
            Action::EnableAiFeatures => {
                let terminal_id = mux.focused_terminal_id();
                let cwd = mux.active_cwd().or_else(|| std::env::current_dir().ok()).unwrap_or_default();
                self.open_panel_with_context(terminal_id, cwd);
            }
            Action::DisableAiFeatures => {
                self.panel_mut().close();
                self.panel_focused = false;
                self.file_picker_focused = false;
            }
            Action::ExplainLastOutput => self.explain_last_output(mux, wakeup_proxy),
            Action::FixLastError      => self.fix_last_error(mux, wakeup_proxy),
        }
    }
}
