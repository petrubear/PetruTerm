use super::App;

impl App {
    /// Drain mux.pending_lua_events, fire each via Lua, drain any queued toast.
    pub(super) fn dispatch_lua_events(&mut self) {
        let events: Vec<&'static str> = std::mem::take(&mut self.mux.pending_lua_events);
        if events.is_empty() {
            return;
        }
        if let Some(lua) = &self.lua {
            for event in events {
                crate::config::lua::fire_lua_event(lua, event);
            }
            if let Some((msg, ms)) = crate::config::lua::drain_lua_toast(lua) {
                self.dispatch_notification(msg, ms);
            }
        }
    }

    /// Fire a single Lua event (terminal_exit, ai_response) and drain any queued toast.
    pub(super) fn fire_lua_event(&mut self, event: &str) {
        if let Some(lua) = &self.lua {
            crate::config::lua::fire_lua_event(lua, event);
            if let Some((msg, ms)) = crate::config::lua::drain_lua_toast(lua) {
                self.dispatch_notification(msg, ms);
            }
        }
    }

    pub(super) fn dispatch_notification(&mut self, msg: String, ms: u64) {
        use crate::config::schema::NotificationStyle;
        match self.config.notifications.style {
            NotificationStyle::Native => {
                crate::platform::notifications::send(&msg);
            }
            NotificationStyle::Toast => {
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
                self.toast = Some((msg, deadline));
                self.request_redraw();
            }
        }
    }

    /// Refresh the cached CWD for the active pane (TD-PERF-02).
    /// Call on PTY data arrival or terminal focus change — NOT every frame.
    pub(super) fn refresh_status_cache(&mut self) {
        self.cached_cwd = self.mux.active_cwd();
    }

    /// Read shell context for a specific terminal and store it by terminal_id.
    /// Skips the disk read when the context file has not changed since last call (TD-PERF-09).
    pub(super) fn update_terminal_shell_ctx(&mut self, terminal_id: usize) {
        let pid = self
            .mux
            .terminals
            .get(terminal_id)
            .and_then(|t| t.as_ref())
            .map(|t| t.child_pid);
        if let Some(pid) = pid {
            let path = crate::llm::shell_context::ShellContext::context_file_path_for_pid(pid);
            let Ok(mtime) = std::fs::metadata(&path).and_then(|m| m.modified()) else {
                return;
            };
            if let Some((_, cached_mtime)) = self.terminal_shell_ctxs.get(&terminal_id) {
                if *cached_mtime == mtime {
                    return;
                }
            }
            if let Some(ctx) = crate::llm::shell_context::ShellContext::load_for_pid(pid) {
                if self.terminal_shell_ctxs.len() >= 256 {
                    // Evict the stale entry (smallest mtime) to keep the map bounded.
                    if let Some(oldest) = self
                        .terminal_shell_ctxs
                        .iter()
                        .min_by_key(|(_, (_, t))| *t)
                        .map(|(id, _)| *id)
                    {
                        self.terminal_shell_ctxs.remove(&oldest);
                    }
                }
                self.terminal_shell_ctxs.insert(terminal_id, (ctx, mtime));
            }
        }
    }

    /// Rebuild the sorted MCP tools cache from the live manager state (AUDIT-PERF-03).
    pub(super) fn rebuild_mcp_cache(&mut self) {
        let mut map: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for (server, tool) in self.ui.mcp_manager.all_tools() {
            map.entry(server).or_default().push(tool.name.clone());
        }
        self.mcp_tools_cache = map.into_iter().collect();
        self.mcp_tools_dirty = false;
    }

    /// Shell context for the currently active pane, if any.
    pub(super) fn active_shell_ctx(&self) -> Option<&crate::llm::shell_context::ShellContext> {
        let tid = self.mux.focused_terminal_id();
        self.terminal_shell_ctxs.get(&tid).map(|(ctx, _)| ctx)
    }

    pub(super) fn check_config_reload(&mut self) {
        const DEBOUNCE_MS: u64 = 300;

        // Global config dir watcher — split lua (app config) vs json (MCP config).
        if let Some(watcher) = &self.config_watcher {
            if let Some(path) = watcher.poll() {
                if path.extension().is_some_and(|e| e == "json") {
                    self.mcp_reload_at = Some(
                        std::time::Instant::now() + std::time::Duration::from_millis(DEBOUNCE_MS),
                    );
                } else {
                    self.config_reload_at = Some(
                        std::time::Instant::now() + std::time::Duration::from_millis(DEBOUNCE_MS),
                    );
                }
            }
        }

        // Project-local .petruterm/ watcher.
        if let Some(watcher) = &self.mcp_watcher {
            if watcher.poll().is_some() {
                self.mcp_reload_at =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(DEBOUNCE_MS));
            }
        }

        // Fire lua config reload.
        if self
            .config_reload_at
            .is_some_and(|t| std::time::Instant::now() >= t)
        {
            self.config_reload_at = None;
            if let Ok((new_cfg, new_lua)) = crate::config::reload() {
                self.config = new_cfg;
                self.lua = Some(new_lua);
                if let Some(rc) = &mut self.render_ctx {
                    rc.renderer
                        .update_bg_color(self.config.colors.background_wgpu());
                }
                self.ui.palette.rebuild_keybinds(&self.config);
                self.ui.palette.rebuild_snippets(&self.config.snippets);
                self.ui.rewire_llm_provider(&self.config);
                log::info!("Config hot-reloaded.");
            }
        }

        // Fire MCP reload.
        if self
            .mcp_reload_at
            .is_some_and(|t| std::time::Instant::now() >= t)
        {
            self.mcp_reload_at = None;
            let cwd = self
                .cached_cwd
                .clone()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_default();
            self.ui.reload_mcp(&cwd);
            self.mcp_tools_dirty = true;
        }
    }
}
