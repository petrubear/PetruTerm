use std::sync::Arc;
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use crate::config::{self, Config};
use crate::llm::chat_panel::{AiEvent, ChatPanel, PanelState};
use crate::llm::LlmProvider;
use crate::llm::shell_context::ShellContext;
use crate::ui::{CommandPalette, SplitDir, Rect};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::Window;
use crate::app::mux::Mux;
use crate::app::renderer::RenderContext;

/// Manages UI overlays like the Chat Panel and Command Palette.
pub struct UiManager {
    pub palette: CommandPalette,
    pub chat_panel: ChatPanel,
    pub panel_focused: bool,
    pub llm_provider: Option<Arc<dyn LlmProvider>>,
    pub tokio_rt: tokio::runtime::Runtime,
    pub ai_tx: Sender<AiEvent>,
    pub ai_rx: Receiver<AiEvent>,
}

impl UiManager {
    pub fn new(config: &Config) -> Self {
        let (ai_tx, ai_rx) = crossbeam_channel::unbounded();
        let tokio_rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");

        let llm_provider = if config.llm.enabled {
            crate::llm::build_provider(&config.llm).ok()
        } else {
            None
        };

        Self {
            palette: CommandPalette::new(),
            chat_panel: ChatPanel::new(),
            panel_focused: false,
            llm_provider,
            tokio_rt,
            ai_tx,
            ai_rx,
        }
    }

    pub fn is_panel_visible(&self) -> bool {
        self.chat_panel.is_visible()
    }

    pub fn poll_ai_events(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.ai_rx.try_recv() {
            changed = true;
            match event {
                AiEvent::Token(tok) => self.chat_panel.append_token(&tok),
                AiEvent::Done => self.chat_panel.mark_done(),
                AiEvent::Error(e) => {
                    log::error!("LLM error: {e}");
                    self.chat_panel.mark_error(e);
                }
            }
        }
        changed
    }

    pub fn submit_ai_query(&mut self, wakeup_proxy: EventLoopProxy<()>) {
        let Some(_user_content) = self.chat_panel.submit_input() else { return };
        let Some(provider) = self.llm_provider.clone() else {
            self.chat_panel.mark_error("LLM not configured".into());
            return;
        };

        let mut system_text = String::from("You are a helpful terminal assistant...");
        if let Some(ctx) = ShellContext::load() {
            system_text.push_str(&format!("\n\nShell context:\n{}", ctx.format_for_system_message()));
        }
        let mut messages = vec![crate::llm::ChatMessage::system(system_text)];
        messages.extend(self.chat_panel.messages.iter().cloned());

        let tx = self.ai_tx.clone();
        let wakeup = wakeup_proxy;

        self.tokio_rt.spawn(async move {
            use futures_util::StreamExt;
            match provider.stream(messages).await {
                Err(e) => { let _ = tx.send(AiEvent::Error(e.to_string())); }
                Ok(mut stream) => {
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(tok) => { let _ = tx.send(AiEvent::Token(tok)); }
                            Err(e) => { let _ = tx.send(AiEvent::Error(e.to_string())); break; }
                        }
                        let _ = wakeup.send_event(());
                    }
                    let _ = tx.send(AiEvent::Done);
                }
            }
            let _ = wakeup.send_event(());
        });
    }

    pub fn explain_last_output(&mut self, mux: &Mux, wakeup_proxy: EventLoopProxy<()>) {
        let output = mux.last_terminal_lines(30);
        if output.is_empty() { return; }
        if !self.chat_panel.is_visible() {
            self.chat_panel.open();
        }
        self.panel_focused = true;
        self.chat_panel.input = format!("Explain this terminal output:\n```\n{}\n```", output);
        self.submit_ai_query(wakeup_proxy);
    }

    pub fn fix_last_error(&mut self, mux: &Mux, wakeup_proxy: EventLoopProxy<()>) {
        let output = mux.last_terminal_lines(30);
        let ctx = ShellContext::load();
        let query = match &ctx {
            Some(c) if !c.last_command.is_empty() => format!("The command `{}` failed (exit code {}). Output:\n```\n{}\n```\nHow do I fix this?", c.last_command, c.last_exit_code, output),
            _ => format!("This command failed. Output:\n```\n{}\n```\nHow do I fix this?", output),
        };
        if !self.chat_panel.is_visible() {
            self.chat_panel.open();
        }
        self.panel_focused = true;
        self.chat_panel.input = query;
        self.submit_ai_query(wakeup_proxy);
    }

    pub fn chat_panel_run_command(&mut self, mux: &mut Mux) {
        if let Some(cmd) = self.chat_panel.last_assistant_command() {
            let mut data = cmd.into_bytes();
            data.push(b'\r');
            if let Some(terminal) = mux.active_terminal() {
                terminal.write_input(&data);
            }
            self.chat_panel.close();
            self.panel_focused = false;
        }
    }

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
            Action::ReloadConfig => if let Ok(new_cfg) = config::reload() {
                *config = new_cfg;
                render_ctx.renderer.update_bg_color(config.colors.background_wgpu());
            },
            Action::OpenConfigFile => { let _ = std::process::Command::new("open").arg(config::config_path()).spawn(); }
            Action::NewTab => {
                let (cols, rows) = mux.active_terminal_size(); // Fallback size
                let (cell_w, cell_h) = (render_ctx.shaper.cell_width as u16, render_ctx.shaper.cell_height as u16);
                let viewport = Rect { x: 0.0, y: 0.0, w: 800.0, h: 600.0 }; // Should be properly calculated
                mux.cmd_new_tab(config, viewport, cols as u16, rows as u16, cell_w, cell_h, wakeup_proxy);
            }
            Action::CloseTab => mux.cmd_close_tab(),
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
            Action::Quit => if let Some(w) = window { let _ = w.request_inner_size(winit::dpi::PhysicalSize::new(0u32, 0u32)); }
            Action::ToggleAiMode | Action::EnableAiFeatures => {
                if !self.chat_panel.is_visible() { self.chat_panel.open(); self.panel_focused = true; }
                else { self.panel_focused = true; }
            }
            Action::DisableAiFeatures => { self.chat_panel.close(); self.panel_focused = false; }
            Action::ExplainLastOutput => self.explain_last_output(mux, wakeup_proxy),
            Action::FixLastError => self.fix_last_error(mux, wakeup_proxy),
        }
    }
}
