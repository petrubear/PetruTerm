// Chat panel slash-command dispatch.

use gpui::Context;

use crate::llm::ChatMessage;

use super::super::GpuiShellRoot;

impl GpuiShellRoot {
    /// Slash-command dispatch. Ported from `src/app/ui/providers.rs`'s
    /// `handle_slash_command` (straight string dispatch plus
    /// `messages.push`) minus the `wakeup_proxy` parameter (the winit wake
    /// has no gpui equivalent). `/skills`/`/mcp` report `skill_manager`/
    /// `mcp_manager`'s real state.
    pub(super) fn handle_slash_command(&mut self, input: &str, cx: &mut Context<Self>) {
        let trimmed = input.trim_start_matches('/');
        let (cmd, args) = trimmed
            .split_once(' ')
            .map_or((trimmed, ""), |(c, a)| (c, a.trim()));

        match cmd {
            "q" | "quit" => {
                self.chat.close(cx);
                return;
            }
            "clear" | "reset" => {
                self.chat.panel.clear_messages();
            }
            "skills" => {
                let skills = self.skill_manager.skills();
                let msg = if skills.is_empty() {
                    "No skills loaded. Place SKILL.md files in ~/.config/petruterm/skills/<name>/"
                        .to_string()
                } else {
                    let filtered: Vec<String> = skills
                        .iter()
                        .filter(|s| {
                            args.is_empty() || s.name.contains(args) || s.description.contains(args)
                        })
                        .map(|s| format!("## {}\n{}", s.name, s.description))
                        .collect();
                    if filtered.is_empty() {
                        format!("No skills matching '{args}'")
                    } else {
                        format!("# Skills\n{}", filtered.join("\n"))
                    }
                };
                self.push_chat_message(msg);
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
                self.push_chat_message(msg);
            }
            "model" => {
                use crate::config::schema::LlmBackend;
                let msg = match self.config.llm.backend {
                    LlmBackend::Agent => "Agent mode: use /agent to switch agents.".to_string(),
                    LlmBackend::Provider if args.is_empty() => {
                        format!(
                            "Active: {}:{}",
                            self.config.llm.provider, self.config.llm.model
                        )
                    }
                    LlmBackend::Provider => {
                        self.config.llm.model = args.to_string();
                        self.chat.rewire_backend(&self.config, &self.tokio_rt);
                        format!("Model set to '{args}'.")
                    }
                };
                self.push_chat_message(msg);
            }
            "agent" => {
                use crate::config::schema::{AcpAgentConfig, LlmBackend};
                let msg = match self.config.llm.backend {
                    LlmBackend::Provider => {
                        "Provider mode active. Use /model to change models.".to_string()
                    }
                    LlmBackend::Agent if args.is_empty() => {
                        match crate::config::llm_view::agent_display_name(
                            self.config.llm.agent.as_ref(),
                        ) {
                            Some(name) => format!("Active agent: {name}"),
                            None => {
                                "No agent configured. Set llm.agent.command in config.".to_string()
                            }
                        }
                    }
                    LlmBackend::Agent => {
                        if let Some(agent_cfg) = self.config.llm.agent.as_mut() {
                            agent_cfg.command = args.to_string();
                        } else {
                            self.config.llm.agent = Some(AcpAgentConfig {
                                command: args.to_string(),
                                args: vec![],
                                env: vec![],
                                display_name: None,
                            });
                        }
                        self.chat.acp_session = None;
                        self.chat.rewire_backend(&self.config, &self.tokio_rt);
                        format!("Agent set to '{args}'. Reconnecting...")
                    }
                };
                self.push_chat_message(msg);
            }
            _ => self.push_chat_message(format!(
                "Unknown command: /{cmd}. Try /clear, /skills, /mcp, /model, /agent or /quit."
            )),
        }
        cx.notify();
    }

    pub(super) fn push_chat_message(&mut self, text: String) {
        self.chat.panel.messages.push(ChatMessage::assistant(text));
        self.chat.panel.dirty = true;
    }
}
