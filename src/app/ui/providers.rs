use super::UiManager;
use crate::config::Config;
use crate::llm::mcp::config as mcp_config;
use crate::llm::mcp::manager::McpManager;

impl UiManager {
    /// Reload MCP servers from disk config. Creates a fresh McpManager, starts all
    /// servers, and replaces the Arc. Called on hot-reload of mcp.json (D-5).
    pub fn reload_mcp(&mut self, cwd: &std::path::Path) {
        let mut cfg = match mcp_config::load_global() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("MCP hot-reload: failed to load global config: {e:#}");
                return;
            }
        };
        // Include local config only if the cwd is trusted (AUDIT-SEC-02).
        let local_path = cwd.join(".petruterm/mcp.json");
        if local_path.exists() {
            if crate::llm::mcp::trust::is_trusted(cwd) {
                match mcp_config::load_local(cwd) {
                    Ok(local) => cfg.extend(local),
                    Err(e) => log::warn!("MCP hot-reload: failed to load local config: {e:#}"),
                }
            } else {
                log::info!(
                    "MCP hot-reload: local config at {} not trusted, skipping.",
                    local_path.display()
                );
            }
        }
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
            let trusted = crate::llm::mcp::trust::is_trusted(&cwd);
            self.skill_manager.load(&cwd, trusted);
            self.steering_manager.load(&cwd, trusted);
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
