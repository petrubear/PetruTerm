// gpui chrome migration (M5a final review): `GpuiShellRoot::new` extracted
// from `mod.rs` to reclaim headroom under the 400-line convention --
// accumulated overshoot from M5a Tasks 1-4, each individually justified but
// never fixed until this final pass. Pure move, no behavior change.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{AppContext, Context};

use crate::config::Config;
use crate::llm::mcp::manager::McpManager;
use crate::llm::mcp::{config as mcp_config, trust};
use crate::llm::skills::SkillManager;
use crate::llm::steering::SteeringManager;
use crate::ui::palette::CommandPalette;
use crate::ui::search_bar::SearchBar;

use super::panes::PaneForest;
use super::spawn_terminal::spawn_terminal;
use super::GpuiShellRoot;
use super::{
    ai_block, chat_panel, context_menu, info_overlay, leader, panes, poll, sidebar, status_bar,
    text_input, workspace,
};

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
        let mut block_managers = HashMap::new();
        block_managers.insert(terminal_id, crate::term::BlockManager::new());

        let leader_map = leader::build_leader_map(
            &crate::config::keybind_view::leader_bindings_view(&config).bindings,
        );

        // Same construction pattern as the wgpu app's own `tokio_rt` field
        // on its `App`/`Mux` struct (`src/app/ui/mod.rs`) -- hoisted into
        // its own binding, rather than built inline in the `Self { .. }`
        // literal below (M3c's shape), because MCP startup needs to
        // `.block_on()` it before the struct exists, and `ChatPanelView::
        // new` (M5a) needs it too, to spawn an initial ACP connect when
        // `config.llm.backend == Agent`.
        let tokio_rt = tokio::runtime::Runtime::new().expect("Failed to build tokio runtime");
        let chat = chat_panel::ChatPanelView::new(cx, &config, &tokio_rt);
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
            block_managers,
            snippet_word: String::new(),
            branch_scan_rx: None,
            pending_send_to_chat: None,
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
            battery: status_bar::BatteryState::default(),
            exit_code: status_bar::ExitCodeState::default(),
            tab_rename: None,
            workspace_rename: None,
            sidebar: sidebar::WorkspaceSidebar::default(),
            sidebar_focus_handle: cx.focus_handle(),
            sidebar_width_px: sidebar::render::DEFAULT_SIDEBAR_WIDTH_PX,
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
            pending_agent_action: None,
            terminal_exit_codes: HashMap::new(),
            terminal_final_output: HashMap::new(),
            pending_acp_wait_for_exit: Vec::new(),
            pending_pty_run: None,
        }
    }
}
