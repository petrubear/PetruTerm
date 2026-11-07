# Graph Report - PetruTerm  (2026-11-06)

## Corpus Check
- 341 files · ~473,052 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 4939 nodes · 9356 edges · 288 communities (268 shown, 20 thin omitted)
- Extraction: 98% EXTRACTED · 2% INFERRED · 0% AMBIGUOUS · INFERRED: 211 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `f53c267b`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Src Config
- Src App
- Src Term
- Src Llm
- Src Llm
- Src Font
- Src Font
- Src Ui
- Src Term
- Src App
- Src Ui
- Src App
- Src Term
- Src Renderer
- Src Llm
- Src Llm
- Src Ui
- Src Term
- Src App
- Src Llm
- Src Config
- Src App
- Benches Build Instances.rs
- Src App
- Src App
- Src Ui
- .context Specs
- Src Llm
- Src Ui
- Src Llm
- Src Llm
- Src Renderer
- Src Llm
- Src Llm
- Src Renderer
- Src App
- Src Llm
- Src Llm
- Src App
- Src Llm
- Src Llm
- Src App
- Src Llm
- Src App
- Src Llm
- Src App
- Src App
- Src App
- Src App
- Src Llm
- Benches Rasterize.rs
- Benches Search.rs
- Config Default
- Src Config
- Src App
- Src Term
- Src Renderer
- WakeupGate
- Src Font
- Src Renderer
- Benches Shaping.rs
- ConfigWatcher
- Src App
- Src Llm
- Scripts Gen Icon.swift
- AcpAgentConfig
- .handle_mouse_button
- Src Llm
- shaper.rs
- Config Default
- Src Term
- Src Renderer
- Src I18n.rs
- Src Main.rs
- Assets Appicon.png
- Scripts Build Pgo.sh
- Scripts Bundle.sh
- Scripts Ci Local.sh
- Src Platform
- Config Default
- Src App
- Src Font
- LcdGlyphAtlas
- shaper.rs
- GRAPH-ARCH-01 First Slice Design
- Global Constraints
- Global Constraints
- .rasterize_lcd_to_atlas
- McpManager
- GRAPH-ARCH-01 Chat Header LLM View Slice Implementation Plan
- Active Context Archive
- SearchBar
- cfdict_str
- keybind_view.rs
- .on_key_down
- .handle_redraw
- RenamePrompt
- shaping.rs
- Mux
- gpu.rs
- resolve_color
- run_session
- Pty
- atlas.rs
- translate_key
- Global Constraints
- Global Constraints
- CellVertex
- RenderContext
- run_session
- AppMenu
- CommandPalette
- spawn_acp_connect
- Config
- .new
- .dispatch_leader_action
- exit_code.rs
- build_font_system
- ExitCodeState
- to_rgba
- gpu.rs
- render_line
- Self
- M3b — AI Chat Panel Implementation Plan
- build_font_system
- M3a — Text Input Primitive Implementation Plan
- Pty
- ResizeHandleElement
- pty_schedule.rs
- to_rgba
- sections.rs
- FreeTypeLcdRasterizer
- Column
- leader.rs
- .collect_grid_cells_for
- spawn_terminal_at
- .handle_redraw
- snapshot.rs
- GpuiShellRoot
- Cell
- M4 — Remaining Surfaces: Design
- build_frame_callbacks
- App
- .new
- Pty
- .open_terminal_for_acp
- M5a — ACP Agent Backend & Tool-Calling Design
- spawn_acp_connect
- M5c — Palette & Context-Menu Feature Completion Design
- M5d — Prompt Context Injection (Skills, Steering, Shell Context, MCP) Design
- .size
- .run_ai_query
- Action
- .handle_sidebar_focused_key
- snippets.rs
- GpuiShellRoot
- M5a — ACP Agent Backend & Tool-Calling Implementation Plan
- M5d — Prompt Context Injection Implementation Plan
- palette.rs
- AppMenu
- WakeupGate
- .maybe_handle_search_key
- status_bar/battery.rs
- full_grid_text
- M5c — Palette & Context-Menu Feature Completion Implementation Plan
- M5b — Chat Composer Extras Design
- .render_chat_drawer
- UiManager
- .maybe_handle_palette_key
- WindowConfig
- gpui M3c: Workspace Layer + Workspace Sidebar Drawer Implementation Plan
- gpui M3d: Sidebar MCP/Skills/Steering Sections + InfoOverlay Implementation Plan
- list_git_branches_sync
- render_header
- .dispatch_palette_action
- .begin_tab_rename
- gpui M4a: Command Palette Implementation Plan
- gpui M4b: Search Bar Implementation Plan
- gpui M4c: Context Menu (Scoped Down) Implementation Plan
- M5b — Chat Composer Extras Implementation Plan
- keybind_view.rs
- .handle_slash_command
- .show_toast
- M4d — Toasts Implementation Plan
- .spawn
- ChatPanelView
- .write_key_to_terminal
- cache.rs
- .new
- draw_glyphs
- BatteryStatus
- claude-stop-ci.sh
- AgentStepResult
- .rasterize_lcd_to_atlas
- cfdict_str
- built_in_actions
- term/agents.rs
- cli.rs
- rasterize_grid
- RemoteOverlay
- term/attention.rs
- Packet
- ipc/mod.rs
- control.rs
- GpuiShellRoot
- shell.rs
- app/renderer/mod.rs
- AgentScanState
- PairingCode
- claude_hooks.rs
- bridge.rs
- Tasks: Agents Sidebar Tab
- Tee
- claude_shim.rs
- Core Principles
- Feature Specification: [FEATURE NAME]
- RecognizedAgent
- hook_cli.rs
- Core Principles
- Research: Agents Sidebar Tab
- build_shaping_spans
- Implementation Plan: [FEATURE]
- Data Model: Agents Sidebar Tab
- GpuiPanes
- String
- mux/remote.rs
- RowRevisionMap
- [CHECKLIST TYPE] Checklist: [FEATURE NAME]
- .ui_style
- .pair_remote
- .render_sidebar_drawer
- Quickstart: validating the Agents sidebar tab
- .route
- .agent_probes
- main
- claude
- config-agents-recognized.md
- sidebar-agents-ui.md
- spawn_terminal_at
- InfoOverlay
- claude_hooks.rs
- Tasks: Agent-to-Agent Messaging
- Research: Agent-to-Agent Messaging
- AgentDetection
- decide.rs
- Pty
- Data Model: Agent-to-Agent Messaging
- FreeTypeCmapLookup
- peer.rs
- Option
- build_frame
- InfoOverlay
- BatteryStatus
- .begin_agent_compose
- McpServerConfig
- run_io_loop
- AgentCompose
- .render_chat_drawer
- messaging.rs
- Messages between agents in PetruTerm
- Messages between agents in PetruTerm
- validate_path
- Quickstart: validating agent-to-agent messaging
- .clear_color
- .agent_probes
- agents-menu-messaging.md
- agents-setup-cli.md
- cli-msg.md
- delivered-frame.md
- socket-msg-protocol.md

## God Nodes (most connected - your core abstractions)
1. `Config` - 96 edges
2. `ColorScheme` - 93 edges
3. `UiManager` - 63 edges
4. `Terminal` - 62 edges
5. `FontConfig` - 54 edges
6. `Mux` - 53 edges
7. `GpuiShellRoot` - 52 edges
8. `App` - 51 edges
9. `ChatPanel` - 50 edges
10. `GpuRenderer` - 50 edges

## Surprising Connections (you probably didn't know these)
- `bench_upload_bytes_comparison()` --calls--> `account_terminal_uploads()`  [INFERRED]
  benches/build_instances.rs → src/renderer/upload.rs
- `bench_upload_bytes_comparison()` --calls--> `merge_upload_ranges()`  [INFERRED]
  benches/build_instances.rs → src/renderer/upload.rs
- `load_or_create_identity()` --calls--> `generate_keypair()`  [INFERRED]
  src/remote/store.rs → crates/remote-proto/src/noise.rs
- `Phase 4 Plugin Ecosystem Focus` --conceptually_related_to--> `Phase 9 UI Restyle Complete`  [AMBIGUOUS]
  AGENTS.md → .context/core/ACTIVE_CONTEXT.md
- `create_headless_wgpu()` --references--> `Device`  [EXTRACTED]
  benches/build_instances.rs → src/remote/store.rs

## Import Cycles
- 1-file cycle: `src/app/renderer/terminal.rs -> src/app/renderer/terminal.rs`
- 1-file cycle: `src/gpui_shell/status_bar/exit_code.rs -> src/gpui_shell/status_bar/exit_code.rs`
- 2-file cycle: `src/font/freetype_lcd.rs -> src/renderer/lcd_atlas.rs -> src/font/freetype_lcd.rs`
- 2-file cycle: `src/gpui_shell/rasterize/grid.rs -> src/term/mod.rs -> src/gpui_shell/rasterize/grid.rs`

## Hyperedges (group relationships)
- **Project Operational Context Set** — context_core_active_context_document, context_core_session_state_document, context_quality_technical_debt_document [INFERRED 0.85]
- **Planning and Specification Backbone** — context_specs_build_phases_document, context_specs_build_phases_archive_document, context_specs_term_specs_document [INFERRED 0.75]
- **Release Artifact Chain** — github_workflows_release_document, changelog_document, readme_document [INFERRED 0.65]

## Communities (288 total, 20 thin omitted)

### Community 0 - "Src Config"
Cohesion: 0.06
Nodes (29): Content Quality, Feature Readiness, Notes, Requirement Completeness, Specification Quality Checklist: Agent-to-Agent Messaging, Complexity Tracking, Constitution Check, Documentation (this feature) (+21 more)

### Community 1 - "Src App"
Cohesion: 0.13
Nodes (8): ChatPanel, ContextMenu, Instant, JoinHandle, Mux, PathBuf, Vec, UiManager

### Community 2 - "Src Term"
Cohesion: 0.06
Nodes (37): command_end_deactivates(), command_start_deactivates(), ctrl_u_clears_before_cursor(), InputShadow, insert(), kill_word(), kill_word_at_start(), kill_word_trailing_spaces() (+29 more)

### Community 3 - "Src Llm"
Cohesion: 0.05
Nodes (40): focus_terminal_clears_zoom_on_other_pane(), focus_terminal_switches_tab_and_pane(), focus_terminal_switches_workspace(), lists_terminal_ids_per_tab_and_overall(), Mux, mux_with_tabs(), Option, Vec (+32 more)

### Community 4 - "Src Llm"
Cohesion: 0.19
Nodes (19): dispatch_response(), extract_tool_result_text(), extract_tool_result_text_content_blocks(), extract_tool_result_text_fallback_to_json(), McpClient, McpTool, OutboundMsg, parse_tools_list() (+11 more)

### Community 5 - "Src Font"
Cohesion: 0.16
Nodes (14): AttrsList, LayoutGlyph, LruCache, build_attr_list(), CellStyle, glyph_to_cache_key(), has_ligature_chars(), is_pua() (+6 more)

### Community 6 - "Src Font"
Cohesion: 0.11
Nodes (20): SectionSelectCallback, render_section_tabs(), AgentEditCancelCallback, AgentMenuCallback, AgentOpenCallback, AnyElement, Div, HashMap (+12 more)

### Community 7 - "Src Ui"
Cohesion: 0.17
Nodes (10): char_chunks(), ChatPanel, Path, PathBuf, String, Vec, scan_dir(), scan_files() (+2 more)

### Community 8 - "Src Term"
Cohesion: 0.10
Nodes (24): Block, block_count_is_capped(), BlockManager, blocks_in_viewport_filters_correctly(), complete_block_lifecycle(), exit_code_nonzero(), incomplete_block_not_in_viewport(), mgr() (+16 more)

### Community 9 - "Src App"
Cohesion: 0.06
Nodes (52): a_pane_that_never_started_the_gesture_is_not_marked_dragged(), a_pane_whose_gesture_already_ended_is_not_marked_dragged_by_a_later_pass_through(), accumulate_scroll_lines(), click_count_caps_at_three(), ClickState, different_cell_resets_to_count_one(), double_and_triple_click_are_dragged_with_zero_movement(), is_dragging_scrollbar() (+44 more)

### Community 10 - "Src Ui"
Cohesion: 0.12
Nodes (16): 1. Instrument the existing path, 2. Coalesce event-loop work, 3. Propagate row revisions, 4. Upload changed ranges, Current Problem, Data Flow, Error Handling and Safety, Execution Tasks (+8 more)

### Community 11 - "Src App"
Cohesion: 0.08
Nodes (52): AtomicU64, CloseCode, admit(), Bucket, client_connection(), close_msg(), handle(), host_connection() (+44 more)

### Community 12 - "Src Term"
Cohesion: 0.17
Nodes (23): Event, EventListener, OnceLock, RawFd, open_pty(), pty_write_all(), PtyEvent, PtyEventProxy (+15 more)

### Community 13 - "Src Renderer"
Cohesion: 0.09
Nodes (18): PresentMode, GpuRenderer, make_main_atlas_bind_group(), render_outcome_requires_rebuild(), RenderOutcome, Arc, BindGroup, Buffer (+10 more)

### Community 14 - "Src Llm"
Cohesion: 0.15
Nodes (20): AcpSession, build_acp_agent(), PromptMsg, AcpAgent, JoinHandle, McpServer, Path, Result (+12 more)

### Community 15 - "Src Llm"
Cohesion: 0.07
Nodes (5): ChatPanel, ConfirmDisplay, Option, String, Vec

### Community 16 - "Src Ui"
Cohesion: 0.05
Nodes (17): Self, section_prev_cycles_backward_and_wraps(), SidebarSection, WorkspaceSidebar, Option, String, SidebarState, label_format_and_truncation() (+9 more)

### Community 17 - "Src Term"
Cohesion: 0.08
Nodes (20): CursorShape, F, CursorInfo, process_cwd(), real_pty_child_notification_sequences_become_notify_events(), real_pty_child_sees_pane_identity_env(), real_pty_remote_subscriber_gets_snapshot_then_live_output(), Arc (+12 more)

### Community 18 - "Src App"
Cohesion: 0.13
Nodes (18): ApplicationHandler, App, Arc, Drop, EventLoopProxy, HashMap, InfoOverlay, Instant (+10 more)

### Community 19 - "Src Llm"
Cohesion: 0.15
Nodes (21): OnceCell, RequestBuilder, CachedJwt, CopilotProvider, CopilotTokenResponse, DeviceCodeResponse, keychain_load(), keychain_save() (+13 more)

### Community 20 - "Src Config"
Cohesion: 0.19
Nodes (25): LuaResult, bytecode_cache_path(), config_stdlib(), drain_lua_toast(), evict_stale_lua_cache(), fire_lua_event(), hash_path(), inject_petruterm_global() (+17 more)

### Community 21 - "Src App"
Cohesion: 0.17
Nodes (11): File Map, Global Constraints, Task 1: Adding baseline performance counters, Task 2: Coalescing PTY wakeups and event-loop drains, Task 3: Propagating explicit dirty rows and revisions, Task 4: Storing terminal instances in stable row slots, Task 5: Adding merged GPU range uploads, Task 6: Proving equivalence and fallback behavior (+3 more)

### Community 22 - "Benches Build Instances.rs"
Cohesion: 0.20
Nodes (16): close_focused_promotes_the_sibling(), close_focused_refuses_to_close_the_last_pane(), close_specific_moves_focus_off_the_closed_pane(), FocusDir, nested_split_produces_three_leaves(), next_node_id(), PaneForest, PaneTree (+8 more)

### Community 23 - "Src App"
Cohesion: 0.14
Nodes (14): InputHandler, ActiveEventLoop, EventLoopProxy, HashMap, Instant, KeyEvent, Modifiers, Mux (+6 more)

### Community 24 - "Src App"
Cohesion: 0.22
Nodes (3): App, Option, Result

### Community 25 - "Src Ui"
Cohesion: 0.06
Nodes (34): GpuiShellRoot, item(), Context, Pixels, Point, Self, ContextMenu, GpuiShellRoot (+26 more)

### Community 26 - ".context Specs"
Cohesion: 0.11
Nodes (25): AGENTS Guide, Phase 4 Plugin Ecosystem Focus, Changelog, CLAUDE Instructions, System Map, Active Context, Phase 9 UI Restyle Complete, Session State (+17 more)

### Community 27 - "Src Llm"
Cohesion: 0.14
Nodes (8): App, Option, String, Option, PathBuf, Self, String, ShellContext

### Community 28 - "Src Ui"
Cohesion: 0.07
Nodes (40): Backspace, Copy, Cut, Delete, End, EntityInputHandler, Home, Left (+32 more)

### Community 29 - "Src Llm"
Cohesion: 0.10
Nodes (22): GpuiShellRoot, App, Arc, ChatPanelView, ContextMenu, Entity, Focusable, FocusHandle (+14 more)

### Community 30 - "Src Llm"
Cohesion: 0.07
Nodes (40): active_skill_continues_when_no_new_match(), attached_file_content_is_injected_with_header(), attached_file_over_cap_gets_truncated(), build_prompt_addendum(), empty_managers_produce_no_skill_or_steering_text(), matching_skill_gets_injected_and_recorded(), no_match_and_no_active_skill_leaves_matched_skill_none(), PromptAddendum (+32 more)

### Community 31 - "Src Renderer"
Cohesion: 0.09
Nodes (20): Device, AtlasEntry, ColorAtlas, dummy_entry(), dummy_key(), evict_cold_keeps_all_when_all_warm(), evict_cold_removes_all_when_all_stale(), evict_cold_removes_old_entries() (+12 more)

### Community 32 - "Src Llm"
Cohesion: 0.10
Nodes (6): ActionPayload, AgentAction, parse_action_from_response(), Option, String, PanelState

### Community 33 - "Src Llm"
Cohesion: 0.19
Nodes (16): Value, build_provider(), ChatRole, infer_context_window(), parse_agent_response(), parse_sse_chunk(), parse_usage(), Arc (+8 more)

### Community 34 - "Src Renderer"
Cohesion: 0.18
Nodes (7): a_reset_or_changed_name_no_longer_routes(), AgentNames, here(), HashMap, Option, String, set_get_reset()

### Community 35 - "Src App"
Cohesion: 0.09
Nodes (23): resolve_line_fg(), RenderContext, ContextMenu, InfoOverlay, Option, StatusBar, ColorScheme, FontConfig (+15 more)

### Community 36 - "Src Llm"
Cohesion: 0.15
Nodes (15): AgentRequest, ApiMessage, build_api_messages(), ChatRequest, OpenAICompatProvider, ApiMessage, Client, Option (+7 more)

### Community 37 - "Src Llm"
Cohesion: 0.23
Nodes (9): AgentStepResult, AgentTool, execute_tool(), Option, Path, String, Value, Vec (+1 more)

### Community 38 - "Src App"
Cohesion: 0.24
Nodes (11): AgentSessions, binds_to_the_process_and_drops_when_it_changes_or_goes(), det(), GpuiShellRoot, process_gone_drops_bound_id(), Reported, HashMap, Instant (+3 more)

### Community 39 - "Src Llm"
Cohesion: 0.20
Nodes (15): command_returns_none_on_empty_panel(), command_returns_none_when_only_tool_lines(), command_strips_done_tool_line(), command_strips_in_progress_tool_line(), command_strips_markdown_fence_after_tool_lines(), command_strips_multiple_tool_lines(), command_without_tool_lines_unchanged(), header_action_for_col() (+7 more)

### Community 40 - "Src Llm"
Cohesion: 0.15
Nodes (16): AgentRequest, ApiMessage, build_api_messages(), ChatRequest, keychain_api_key(), OpenRouterProvider, ApiMessage, Client (+8 more)

### Community 41 - "Src App"
Cohesion: 0.14
Nodes (19): App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId (+11 more)

### Community 42 - "Src Llm"
Cohesion: 0.07
Nodes (25): ai_block_body(), ai_block_hint(), AiBlockView, GpuiShellRoot, render_ai_block(), App, Arc, Context (+17 more)

### Community 43 - "Src App"
Cohesion: 0.29
Nodes (6): Global Constraints, GRAPH-ARCH-01 Remaining Domains Slice Implementation Plan, Self-Review, Task 1: Add `LeaderBindingsView` and migrate its two consumers, Task 2: Consolidate the duplicated scaled-font-with-LCD-fixup sequence, Task 3: Consolidate the duplicated `max_fps` interval formula

### Community 44 - "Src Llm"
Cohesion: 0.22
Nodes (5): ElementState, MouseButton, SeparatorDragState, KeyEvent, Option

### Community 45 - "Src App"
Cohesion: 0.15
Nodes (19): PanelMsgParams, RenderContext, ChatPanel, String, brighten(), build_usage_hint(), calculate_row_hash(), colors_approx_eq() (+11 more)

### Community 46 - "Src App"
Cohesion: 0.36
Nodes (12): bench_rasterize_glyph_ascii(), bench_rasterize_line_ascii(), bench_rasterize_line_ligatures(), bench_rasterize_line_unicode(), make_colors(), make_shaper(), rasterize_one(), CacheKey (+4 more)

### Community 47 - "Src App"
Cohesion: 0.07
Nodes (30): Cancel, EventEmitter, SharedString, ChatPanelView, GpuiShellRoot, Context, Entity, GpuiShellRoot (+22 more)

### Community 48 - "Src App"
Cohesion: 0.21
Nodes (8): fetch_git_branch(), list_git_branches_sync(), Duration, Option, Path, String, Vec, UiManager

### Community 49 - "Src Llm"
Cohesion: 0.33
Nodes (9): Path, Self, compress_diff(), diff_lines(), DiffKind, DiffLine, lcs_diff(), String (+1 more)

### Community 50 - "Benches Rasterize.rs"
Cohesion: 0.14
Nodes (19): PaintQuad, PrepaintState, App, Bounds, Element, ElementId, Entity, GlobalElementId (+11 more)

### Community 51 - "Benches Search.rs"
Cohesion: 0.41
Nodes (12): bench_search_cold(), bench_search_cold_par(), bench_search_incremental(), build_flat_grid(), build_grid(), filter_matches(), push_search_match(), Criterion (+4 more)

### Community 53 - "Src Config"
Cohesion: 0.25
Nodes (19): env_vars_parsed(), load_from_paths(), load_global(), load_local(), load_merged(), load_merged_excludes_local_when_untrusted(), load_merged_includes_local_when_trusted(), load_merged_with_no_local_file_returns_global_only() (+11 more)

### Community 54 - "Src App"
Cohesion: 0.24
Nodes (9): detects_absolute_path(), detects_stack_trace(), detects_url(), HoverLink, HoverLinkKind, is_boundary(), Option, String (+1 more)

### Community 55 - "Src Term"
Cohesion: 0.41
Nodes (12): cargo(), curl(), docker(), find(), git(), grep(), kubectl(), lookup_flag() (+4 more)

### Community 56 - "Src Renderer"
Cohesion: 0.33
Nodes (7): BindGroupLayout, CellPipeline, CellPipelineBgAware, CellPipelineLcd, RenderPipeline, Self, TextureFormat

### Community 57 - "WakeupGate"
Cohesion: 0.22
Nodes (17): render_agent_action_card(), render_awaiting_confirm_card(), render_confirm_card(), render_diff_line(), IntoElement, render_chat_panel(), render_message(), render_message_body_lines() (+9 more)

### Community 58 - "Src Font"
Cohesion: 0.35
Nodes (6): FontLocator, FontPath, Default, Option, PathBuf, Self

### Community 59 - "Src Renderer"
Cohesion: 0.18
Nodes (8): RectUniforms, RoundedRectPipeline, BindGroup, Buffer, Queue, RenderPipeline, Self, TextureFormat

### Community 60 - "Benches Shaping.rs"
Cohesion: 0.18
Nodes (22): Keystroke, alt_character_with_meta_prefixes_esc(), alt_character_without_meta_sends_composed_char(), arrow_up_app_cursor_mode_sends_ss3(), arrow_up_normal_mode_sends_csi(), backspace_sends_del(), ctrl_c_sends_control_byte(), ctrl_space_sends_nul() (+14 more)

### Community 61 - "ConfigWatcher"
Cohesion: 0.18
Nodes (12): RecommendedWatcher, ConfigChanges, ConfigFileKind, ConfigWatcher, poll_coalesces_multiple_events_of_the_same_kind(), poll_reports_lua_changed_without_swallowing_a_json_change_right_behind_it(), Duration, Option (+4 more)

### Community 62 - "Src App"
Cohesion: 0.11
Nodes (21): PaneForest, closing_a_background_workspace_after_active_leaves_active_index_unchanged(), closing_a_background_workspace_before_active_keeps_the_same_workspace_active(), closing_an_unknown_workspace_id_is_refused(), closing_the_active_workspace_clamps_to_the_new_last_index(), closing_the_only_remaining_workspace_is_refused(), new_workspace_creates_and_activates_it(), next_and_prev_workspace_wrap_around() (+13 more)

### Community 63 - "Src Llm"
Cohesion: 0.50
Nodes (4): ChatMessage, Into, Self, String

### Community 64 - "Scripts Gen Icon.swift"
Cohesion: 0.31
Nodes (8): CGColor, CGFloat, CoreGraphics, Foundation, ImageIO, hex(), srgb(), UInt32

### Community 65 - "AcpAgentConfig"
Cohesion: 0.36
Nodes (4): llm_runtime_view(), llm_runtime_view_agent_path_requires_agent_config(), llm_runtime_view_preserves_backend_agent_and_ui_width(), llm_runtime_view_preserves_provider_defaults()

### Community 66 - ".handle_mouse_button"
Cohesion: 0.12
Nodes (21): closing_a_background_tab_after_active_leaves_active_index_unchanged(), closing_a_background_tab_before_active_keeps_the_same_tab_active(), closing_the_active_tab_still_clamps_to_the_new_last_index(), label_format_and_truncation(), move_tab_into_an_adjacent_gap_is_a_no_op(), move_tab_keeps_the_active_tab_active_across_the_reorder(), move_tab_reorders_by_dropping_into_the_target_gap(), move_tab_to_gap_zero_puts_it_first() (+13 more)

### Community 67 - "Src Llm"
Cohesion: 0.39
Nodes (7): is_trusted(), Option, Path, PathBuf, Result, trust(), trust_file()

### Community 68 - "shaper.rs"
Cohesion: 0.11
Nodes (39): ClientResult, MaybeTlsStream, ask(), client_task(), CountGuard, CountGuard<'a>, Ctx, handle_packet() (+31 more)

### Community 69 - "Config Default"
Cohesion: 0.40
Nodes (5): fetch, filesystem, npx, @modelcontextprotocol/server-fetch, @modelcontextprotocol/server-filesystem

### Community 70 - "Src Term"
Cohesion: 0.17
Nodes (16): full_and_incremental_row_storage_are_equivalent(), layout_geometry_changes_require_rebuild(), lcd_slots_track_their_own_lengths(), row_slots_are_non_overlapping_and_bounded(), row_storage_equivalent(), RowSlot, RowWriteError, Option (+8 more)

### Community 71 - "Src Renderer"
Cohesion: 0.15
Nodes (21): config_dir(), config_path(), ensure_default_configs(), extract_lua_version(), extract_version(), install_shell_integration(), list_themes(), load() (+13 more)

### Community 72 - "Src I18n.rs"
Cohesion: 0.67
Nodes (3): detect_locale(), init(), String

### Community 73 - "Src Main.rs"
Cohesion: 0.83
Nodes (3): inherit_login_shell_env(), main(), Result

### Community 74 - "Assets Appicon.png"
Cohesion: 1.00
Nodes (3): PetruTerm App Icon, Cursor Block, Terminal Prompt Chevron

### Community 76 - "Scripts Bundle.sh"
Cohesion: 0.67
Nodes (3): build_bundle(), CARGO_TARGET_DIR, bundle.sh script

### Community 77 - "Scripts Ci Local.sh"
Cohesion: 0.50
Nodes (3): CARGO_TARGET_DIR, RUSTFLAGS, ci-local.sh script

### Community 78 - "Src Platform"
Cohesion: 0.80
Nodes (4): native_available(), send(), send_from_unbundled_process_does_not_abort(), send_with()

### Community 87 - "Src App"
Cohesion: 0.20
Nodes (10): IntoIterator, Item, Alert, AttentionTracker, PaneAttention, HashMap, Instant, Option (+2 more)

### Community 88 - "Src Font"
Cohesion: 0.08
Nodes (23): Architecture, Architecture decision: port the pane-tree algorithms, replace the rect math with taffy flex, Explicitly out of scope / not being built, File organization, gpui Chrome Migration — Design, Guiding principles, Input handling, Leader-key chorded dispatch (+15 more)

### Community 92 - "LcdGlyphAtlas"
Cohesion: 0.06
Nodes (36): App, blink_only_render(), blink_overlay_slot(), build_all_pane_instances(), production_overlay_upload_plan_keeps_cursor_first_and_terminal_ranges_separate(), production_upload_fallback_requests_full_rebuild_for_all_visible_rows(), ActiveEventLoop, Duration (+28 more)

### Community 93 - "shaper.rs"
Cohesion: 0.24
Nodes (4): classify_frame_scenario(), FrameMetrics, FrameScenario, upload_metrics_accumulate_and_reset()

### Community 94 - "GRAPH-ARCH-01 First Slice Design"
Cohesion: 0.14
Nodes (13): 1. Introduce a narrow LLM config view API, 2. Migrate one consumer path, 3. Keep behavior unchanged, Current Problem, Data Flow, Error Handling, Goal, GRAPH-ARCH-01 First Slice Design (+5 more)

### Community 95 - "Global Constraints"
Cohesion: 0.25
Nodes (7): Global Constraints, GRAPH-ARCH-01 First Slice Implementation Plan, Self-Review, Task 1: Add a narrow LLM runtime view module, Task 2: Add regression tests for provider-oriented defaults in the view, Task 3: Migrate `UiManager` rewire flow to consume `LlmRuntimeView`, Task 4: Final consistency pass and docs alignment

### Community 96 - "Global Constraints"
Cohesion: 0.29
Nodes (6): Global Constraints, GRAPH-ARCH-01 LLM Domain Closure Slice Implementation Plan, Self-Review, Task 1: Add `agent_display_name` helper to the LLM view module, Task 2: Migrate `handle_slash_command`'s `"model"` and `"agent"` arms to the view, Task 3: Deduplicate `build_panel_header`'s agent-name derivation

### Community 97 - ".rasterize_lcd_to_atlas"
Cohesion: 0.16
Nodes (10): Display, Fn, Result, NameError, Option, Result, String, sanitize_session() (+2 more)

### Community 98 - "McpManager"
Cohesion: 0.11
Nodes (20): body(), noise_between_sequences(), osc777_title_and_body_with_semicolons(), OscNotification, OscNotifyScanner, other_osc_ignored_and_does_not_break_following_ones(), oversized_body_is_capped(), parse() (+12 more)

### Community 99 - "GRAPH-ARCH-01 Chat Header LLM View Slice Implementation Plan"
Cohesion: 0.40
Nodes (4): Global Constraints, GRAPH-ARCH-01 Chat Header LLM View Slice Implementation Plan, Self-Review, Task 1: Migrate `build_panel_header` to `LlmRuntimeView`

### Community 100 - "Active Context Archive"
Cohesion: 0.50
Nodes (3): Active Context Archive, Archive Run — 2026-07-25, Historical snapshot archived from Active Context

### Community 101 - "SearchBar"
Cohesion: 0.20
Nodes (9): Global Constraints, M2 — Core Chrome Implementation Plan, M2 Exit Criteria, Task 1: Pane tree — port algorithms onto a rect-cache instead of cached `Rect`, Task 2: Tabs — `TabManager` port + tab bar UI, `GpuiShellRoot` restructured to be tab-indexed, Task 3: Multi-pane rendering via taffy flex, separator drag, zoom, click-to-focus/click-to-switch-tab, Task 4: Leader-key chorded dispatch, Task 5: Status bar (+1 more)

### Community 102 - "cfdict_str"
Cohesion: 0.15
Nodes (29): empty_message_round_trips(), generate_keypair(), handshake(), handshake_to_the_wrong_host_key_fails(), initiate(), Initiator, Keypair, large_messages_are_chunked_and_reassembled() (+21 more)

### Community 103 - "keybind_view.rs"
Cohesion: 0.12
Nodes (15): 1. `keybind_style` config property and schema, 2. One `keybinds.lua`, both tables, switch at the top, 3. Direct-bind resolution (`config::keybind_view`), 4. Dispatch wiring, per binary, 5. Proposed default "normal" style bindings, Current Problem, Data Flow, Dual Keybind Style Design (+7 more)

### Community 104 - ".on_key_down"
Cohesion: 0.20
Nodes (8): arrow_key_to_focus_dir(), GpuiShellRoot, Context, FocusDir, KeyDownEvent, Option, Self, Window

### Community 105 - ".handle_redraw"
Cohesion: 0.08
Nodes (36): blend_pixel(), inverse_and_selection_compose(), inverse_flag_swaps_fg_bg(), plain_cell_resolves_fg_bg_unswapped(), resolve_cell_colors(), AnsiColor, Flags, FxHashMap (+28 more)

### Community 106 - "RenamePrompt"
Cohesion: 0.12
Nodes (20): DraggedTab, render_tab_bar(), AnyElement, Context, Div, IntoElement, Option, Render (+12 more)

### Community 107 - "shaping.rs"
Cohesion: 0.53
Nodes (9): bench_shape_line_ascii(), bench_shape_line_ascii_cached(), bench_shape_line_ligatures(), bench_shape_line_ligatures_cached(), bench_shape_line_unicode(), make_colors(), make_shaper(), Criterion (+1 more)

### Community 108 - "Mux"
Cohesion: 0.14
Nodes (21): FnMut, SelectionRange, cell_in_selection(), drain_pty_events(), FlagHintOverlay, GhostOverlay, production_pty_drain_preserves_payload_and_special_events_across_budget(), PtyEventBatch (+13 more)

### Community 109 - "gpu.rs"
Cohesion: 0.08
Nodes (18): GridVisualState, RenderContext, RowCache, RowCacheEntry, Arc, Color, HashMap, HashSet (+10 more)

### Community 110 - "resolve_color"
Cohesion: 0.06
Nodes (36): StatusBarColors, clears_cache_when_exit_code_returns_to_zero(), ExitCodeState, loads_nonzero_exit_code_and_skips_reload_when_mtime_unchanged(), Option, Path, PathBuf, SystemTime (+28 more)

### Community 111 - "run_session"
Cohesion: 0.20
Nodes (9): Global Constraints, M1b Exit Criteria, M1b (Grid Parity) Implementation Plan, Task 1: Split `font_state.rs` out of `terminal_element.rs`, Task 2: ANSI colors (fg/bg + bold/italic) + `rasterize.rs` extraction, Task 3: Cursor shapes + blink, Task 4: `mouse.rs` — click-drag selection, copy, click-to-focus, Task 5: Mouse-report passthrough (+1 more)

### Community 112 - "Pty"
Cohesion: 0.11
Nodes (30): FnOnce, collect_primary_face_ids(), compute_cell_size(), font_family(), font_features(), font_size(), FontState, header_row_min_height() (+22 more)

### Community 113 - "atlas.rs"
Cohesion: 0.12
Nodes (27): ProbeResult, command_output(), descendants(), fetch_pr(), focus_change_refreshes_before_the_interval(), git_branch(), parse_lsof_listen(), parse_pr() (+19 more)

### Community 114 - "translate_key"
Cohesion: 0.44
Nodes (9): format_csi(), format_fkey(), format_tilde(), Key, Modifiers, Option, TermMode, Vec (+1 more)

### Community 115 - "Global Constraints"
Cohesion: 0.22
Nodes (8): Global Constraints, gpui Chrome Migration — M0 (Foundation Spike) Implementation Plan, M0 Exit Criteria, Task 1: Expose PetruTerm's existing modules to a shared library crate; add the `gpui-petruterm` binary target, Task 2: Boot a blank gpui window, Task 3: Render one live shell terminal through a custom `TerminalGridElement`, Task 4: Verify the two named risks from the spec — repaint reliability and the ligature-width bug, Task 5: Prove leader-key chorded dispatch reaches real business logic

### Community 116 - "Global Constraints"
Cohesion: 0.25
Nodes (7): Global Constraints, gpui Migration M1a (Foundation Fixes) Implementation Plan, M1a Exit Criteria, Task 1: Load the real config at startup (no hot-reload yet), Task 2: Real font-metrics-driven cell sizing, Task 3: Event-driven repaint (investigation + implementation), Task 4: Config hot-reload

### Community 117 - "CellVertex"
Cohesion: 0.16
Nodes (13): fetch_git_branch(), GitBranchState, invalidate_forces_refetch_within_ttl(), poll_git_branch(), recover_stuck_in_flight(), Duration, Instant, Option (+5 more)

### Community 118 - "RenderContext"
Cohesion: 0.17
Nodes (6): Option, String, Vec, SearchBar, SearchMatch, truncated_count_label_shows_plus_suffix()

### Community 119 - "run_session"
Cohesion: 0.36
Nodes (7): AcpAgent, McpServer, PathBuf, Receiver, Result, Vec, run_session()

### Community 120 - "AppMenu"
Cohesion: 0.25
Nodes (10): HighlightStyle, char_range_to_byte_range(), heading_size(), render_line(), Div, Option, Pixels, span_highlight() (+2 more)

### Community 121 - "CommandPalette"
Cohesion: 0.10
Nodes (11): render_command_palette(), Entity, IntoElement, ScrollHandle, TextInput, CommandPalette, Option, Self (+3 more)

### Community 122 - "spawn_acp_connect"
Cohesion: 0.28
Nodes (15): list_saved_workspaces(), load_workspace(), PaneNodeSnapshot, Box, Option, PathBuf, Result, String (+7 more)

### Community 123 - "Config"
Cohesion: 0.15
Nodes (20): answer_once(), Bus, disabled_bus_answers_unsupported_immediately(), enabled_bus_returns_the_ui_threads_answer(), global(), leak(), MsgRequest, no_answer_times_out() (+12 more)

### Community 124 - ".new"
Cohesion: 0.13
Nodes (24): MutexGuard, builtin_relay_serves_a_pairing_without_any_other_process(), input_to_a_pane_that_was_not_attached_is_refused(), offer(), pair_list_attach_stream_and_type(), Phone, read_only_device_cannot_type_and_unattached_panes_refuse_input(), recv_raw() (+16 more)

### Community 125 - ".dispatch_leader_action"
Cohesion: 0.29
Nodes (6): GpuiShellRoot, Context, IntoElement, Render, Self, Window

### Community 126 - "exit_code.rs"
Cohesion: 0.12
Nodes (15): 1. What the survey changed about the plan, 2. Slicing, 3.1 Text input: port gpui's own reference implementation, as an `Entity`, 3.2 Markdown: native gpui layout for messages, fixed-width for the input, 3.3 Layout: flex siblings, never a manual viewport rect, 3.4 Drawers, animated, 3.5 AI streaming: a channel owned by the shell root, drained in the poll loop, 3.6 Workspace layer mirrors `Mux` (+7 more)

### Community 127 - "build_font_system"
Cohesion: 0.08
Nodes (25): ChatPanelView, GpuiShellRoot, App, Arc, ChatPanel, Context, Entity, GpuiShellRoot (+17 more)

### Community 128 - "ExitCodeState"
Cohesion: 0.17
Nodes (11): Corrections made to the spec during planning, Dual Keybind Style (Data Layer + wgpu Binary) Implementation Plan, Follow-up (not in this plan), Global Constraints, Task 1: `KeybindStyle` config schema + Lua parsing + action whitelist, Task 2: `Mods` parser in `keybind_view.rs`, Task 3: `direct_bindings_view` in `keybind_view.rs`, Task 4: `keybinds.lua` — both tables, style switch, version bump (+3 more)

### Community 129 - "to_rgba"
Cohesion: 0.14
Nodes (17): App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId (+9 more)

### Community 130 - "gpu.rs"
Cohesion: 0.08
Nodes (18): Column, Line, eventloop_wakeup(), Mux, EventLoopProxy, FocusDir, HashMap, Option (+10 more)

### Community 131 - "render_line"
Cohesion: 0.14
Nodes (17): Metrics, Buffer, CacheKey, ID, PathBuf, Queue, Rc, RefCell (+9 more)

### Community 132 - "Self"
Cohesion: 0.12
Nodes (30): agent_display_name(), LlmRuntimeView, Option, AcpAgentConfig, AgentsConfig, BatterySaverMode, ChatUiConfig, Config (+22 more)

### Community 133 - "M3b — AI Chat Panel Implementation Plan"
Cohesion: 0.20
Nodes (9): Dogfood (after all three tasks), File Structure, Global Constraints, M3b — AI Chat Panel Implementation Plan, Scope, Self-Review, Task 1: Panel shell — drawer, layout, markdown, Task 2: Streaming and slash commands (+1 more)

### Community 134 - "build_font_system"
Cohesion: 0.23
Nodes (10): base_family(), build_font_system(), locate_font_for_lcd(), register_variable_weights(), FontSystem, ID, Option, PathBuf (+2 more)

### Community 135 - "M3a — Text Input Primitive Implementation Plan"
Cohesion: 0.29
Nodes (6): File Structure, Global Constraints, M3a — Text Input Primitive Implementation Plan, Self-Review, Task 1: The `TextInput` primitive, Task 2: Wire `RenameTab`, the primitive's first consumer

### Community 136 - "Pty"
Cohesion: 0.15
Nodes (14): a_long_pause_starts_a_new_conversation(), Conversations, Convo, eleventh_message_of_a_conversation_is_refused(), key(), prune_drops_idle_pairs(), rate_limit_allows_ten_then_refuses_with_retry(), rate_limiter_goes_idle() (+6 more)

### Community 137 - "ResizeHandleElement"
Cohesion: 0.13
Nodes (17): ResizeDragCallback, ResizeHandleElement, ResizeHandleId, App, Bounds, Element, ElementId, GlobalElementId (+9 more)

### Community 138 - "pty_schedule.rs"
Cohesion: 0.15
Nodes (8): Result, validate_upload_range(), account_terminal_uploads(), merge_upload_ranges(), Vec, TerminalUploadAccounting, upload_ranges_bytes(), UploadRange

### Community 139 - "to_rgba"
Cohesion: 0.09
Nodes (15): check-prerequisites.sh script, check_dir(), check_file(), get_feature_paths(), get_repo_root(), has_jq(), _persist_feature_json(), resolve_specify_init_dir() (+7 more)

### Community 140 - "sections.rs"
Cohesion: 0.10
Nodes (30): Rgba, to_rgba(), GpuiShellRoot, Context, IntoElement, Self, render_workspace_sidebar(), detail_lines() (+22 more)

### Community 141 - "FreeTypeLcdRasterizer"
Cohesion: 0.14
Nodes (15): FT_Bitmap, FreeTypeLcdRasterizer, Drop, FT_Face, FT_Library, HashMap, Mutex, Option (+7 more)

### Community 142 - "Column"
Cohesion: 0.43
Nodes (5): block_output_text_for(), GpuiShellRoot, row_text_and_absolute_row(), Option, String

### Community 143 - "leader.rs"
Cohesion: 0.20
Nodes (6): GpuiShellRoot, home_str(), Context, Result, Self, String

### Community 144 - ".collect_grid_cells_for"
Cohesion: 0.06
Nodes (32): Content Quality, Feature Readiness, Notes, Requirement Completeness, Specification Quality Checklist: Agents Sidebar Tab, Amendment 2026-11-06: session id and custom names (US5, US6), Complexity Tracking, Constitution Check (+24 more)

### Community 145 - "spawn_terminal_at"
Cohesion: 0.19
Nodes (19): a_users_own_file_is_never_touched(), an_older_managed_file_is_updated_with_a_backup(), apply(), backup_path(), dry_run_writes_nothing(), fresh_install_creates_both_and_second_run_changes_nothing(), install(), is_managed() (+11 more)

### Community 146 - ".handle_redraw"
Cohesion: 0.16
Nodes (27): accept_loop(), delivers_valid_lines_and_skips_garbage(), handle_connection(), msg_requests_get_an_answer_and_hooks_still_queue(), oversized_line_drops_the_connection(), remove_stale_sockets(), Arc, Fn (+19 more)

### Community 147 - "snapshot.rs"
Cohesion: 0.10
Nodes (29): Dimensions, TermSize, alt_screen_modes_and_hidden_cursor(), assert_round_trip(), blank_cells_with_background_survive(), colors_and_attributes(), empty_screen(), feed() (+21 more)

### Community 148 - "GpuiShellRoot"
Cohesion: 0.22
Nodes (5): GpuiShellRoot, App, Context, Self, SplitDir

### Community 149 - "Cell"
Cohesion: 0.21
Nodes (26): add_device(), clear_status(), devices_add_find_revoke(), dir(), ensure_dir(), find_device(), identity_is_created_once_and_private(), load_devices() (+18 more)

### Community 150 - "M4 — Remaining Surfaces: Design"
Cohesion: 0.14
Nodes (13): 1. Scope, 2. What's reusable vs. new, 3. M4a — Command Palette, 4.1 UI, 4.2 Search execution — the real decision, 4.3 Rendering matches, 4. M4b — Search Bar, 5. M4c — Context Menu (Scoped Down) (+5 more)

### Community 151 - "build_frame_callbacks"
Cohesion: 0.17
Nodes (15): FileDropCallback, build_file_drop_callback(), build_frame_callbacks(), build_tab_reorder_callback(), build_tab_right_click_callback(), ChatPillCallback, Context, ContextActionCallback (+7 more)

### Community 152 - "App"
Cohesion: 0.12
Nodes (15): 1. Attention state (shared core), 2. PTY parsing: OSC notifications, 3. Control socket + `petruterm notify`, 4. Zero-setup agent wrapper (slice 3), and explicit installer, 5. UI, 6. Config (`notifications.lua`), Agent Attention Layer - Design, cmux findings (read from the cmux sources, 2026-10-15) (+7 more)

### Community 153 - ".new"
Cohesion: 0.23
Nodes (21): AnnotatedLine, BlockKind, code_fence_syntax_highlight(), emit_list_item(), fence_state_carries_across_calls(), find_close(), headings(), highlight_code() (+13 more)

### Community 154 - "Pty"
Cohesion: 0.62
Nodes (5): filter_matches(), push_search_match(), SearchMatch, Vec, search_terminal()

### Community 155 - ".open_terminal_for_acp"
Cohesion: 0.23
Nodes (7): GpuiShellRoot, Context, Option, PathBuf, Self, String, shell_quote()

### Community 156 - "M5a — ACP Agent Backend & Tool-Calling Design"
Cohesion: 0.17
Nodes (11): 10. Manual testing required (cannot be verified from the agent sandbox), 1. Overview, 2. Global Constraints, 3. Task 1 -- Inline-action confirm flow (no ACP dependency), 4. Task 2 -- Terminal-bridge foundation: exit codes + final output, 5. Task 3 -- ACP session lifecycle + backend switching, 6. Task 4 -- ACP terminal bridge, 7. Task 5 -- ACP prompt submission + tool-status streaming (+3 more)

### Community 157 - "spawn_acp_connect"
Cohesion: 0.24
Nodes (7): ChatPanelView, PathBuf, Receiver, Result, Runtime, String, spawn_acp_connect()

### Community 158 - "M5c — Palette & Context-Menu Feature Completion Design"
Cohesion: 0.18
Nodes (10): 1. Overview, 2. Global Constraints, 3. Task 1 — Command blocks, 4. Task 2 — Snippets, 5. Task 3 — Hover-link detection, 6. Task 4 — Git-branch picker, 7. Task 5 — Saved workspaces, 8. Deferred (recorded, not reopened for reconsideration here) (+2 more)

### Community 159 - "M5d — Prompt Context Injection (Skills, Steering, Shell Context, MCP) Design"
Cohesion: 0.18
Nodes (10): 1. Overview, 2. Global Constraints, 3. The shared prompt-context builder, 4. MCP config loading, consolidated, 5. ACP session wiring, 6. Consumer wiring at each call site, 7. Testing, 8. Manual testing required (cannot be verified from the agent sandbox) (+2 more)

### Community 160 - ".size"
Cohesion: 0.20
Nodes (7): main(), quit(), App, spawn_config_watcher(), Context, GpuiShellRoot, spawn_poll_loop()

### Community 161 - ".run_ai_query"
Cohesion: 0.35
Nodes (7): GpuiShellRoot, last_terminal_lines_for(), Context, Option, Self, String, Window

### Community 162 - "Action"
Cohesion: 0.18
Nodes (5): MouseScrollDelta, PhysicalPosition, ActiveEventLoop, WindowEvent, WindowId

### Community 163 - ".handle_sidebar_focused_key"
Cohesion: 0.32
Nodes (5): GpuiShellRoot, Context, KeyDownEvent, Self, Window

### Community 164 - "snippets.rs"
Cohesion: 0.20
Nodes (6): GpuiShellRoot, Context, KeyDownEvent, Self, String, try_expand_snippet()

### Community 165 - "GpuiShellRoot"
Cohesion: 0.36
Nodes (5): GpuiShellRoot, Context, KeyDownEvent, Self, Window

### Community 166 - "M5a — ACP Agent Backend & Tool-Calling Implementation Plan"
Cohesion: 0.20
Nodes (9): Exit Criteria, Global Constraints, M5a — ACP Agent Backend & Tool-Calling Implementation Plan, Task 1: Inline-action confirm flow, Task 2: Terminal-bridge foundation -- exit codes + final output, Task 3: ACP session lifecycle + backend switching, Task 4: ACP terminal bridge, Task 5: ACP prompt submission + tool-status streaming (+1 more)

### Community 167 - "M5d — Prompt Context Injection Implementation Plan"
Cohesion: 0.20
Nodes (9): Global Constraints, M5d — Prompt Context Injection Implementation Plan, Manual testing checklist (spec §8, reproduce after all 6 tasks land), Task 1: The shared prompt-context builder, Task 2: MCP config consolidation, Task 3: ACP session wiring — native `mcp_servers`, Task 4: wgpu — MCP config consolidation + `mcp_servers` wiring, Task 5: wgpu — prompt-context wiring in `submit_ai_query` (+1 more)

### Community 168 - "palette.rs"
Cohesion: 0.37
Nodes (20): apply_row_offset(), bench_build_frame_dirty_rows(), bench_build_frame_hit(), bench_build_frame_hit_large_par(), bench_build_frame_hit_large_serial(), bench_build_frame_miss(), bench_build_row_hit(), bench_build_row_miss() (+12 more)

### Community 169 - "AppMenu"
Cohesion: 0.07
Nodes (27): FromStr, Menu, MenuEvent, MenuId, AppMenu, Option, Self, Vec (+19 more)

### Community 170 - "WakeupGate"
Cohesion: 0.27
Nodes (7): gate_sends_once_until_drain(), Arc, AtomicBool, Self, signal_during_drain_is_not_lost(), take_pending_clears_and_reports(), WakeupGate

### Community 171 - ".maybe_handle_search_key"
Cohesion: 0.27
Nodes (6): GpuiShellRoot, App, Context, KeyDownEvent, Self, Window

### Community 172 - "status_bar/battery.rs"
Cohesion: 0.17
Nodes (10): BatteryState, does_not_poll_before_ttl_expires(), poll_battery(), polls_again_once_ttl_expires(), resolve_battery_saver_active(), Duration, Instant, Option (+2 more)

### Community 173 - "full_grid_text"
Cohesion: 0.20
Nodes (10): evict_oldest_closed_terminals(), eviction_drops_the_oldest_entry_first(), eviction_keeps_maps_at_the_cap(), full_grid_text(), GpuiShellRoot, recording_the_same_terminal_twice_does_not_duplicate_the_order_entry(), HashMap, Option (+2 more)

### Community 174 - "M5c — Palette & Context-Menu Feature Completion Implementation Plan"
Cohesion: 0.22
Nodes (8): Exit Criteria, Global Constraints, M5c — Palette & Context-Menu Feature Completion Implementation Plan, Task 1: Command blocks, Task 2: Snippets, Task 3: Hover-link detection, Task 4: Git-branch picker, Task 5: Saved workspaces

### Community 175 - "M5b — Chat Composer Extras Design"
Cohesion: 0.22
Nodes (8): 1. Overview, 2. Global Constraints, 3. Task 1 -- `Leader a e` / `Leader a f` + palette entries, 4. Task 2 -- Suggestion pills (zero-state + post-response), 5. Task 3 -- File attachment picker, 6. Deferred (recorded, not reopened for reconsideration here), 7. Manual testing required (cannot be verified from the agent sandbox), M5b — Chat Composer Extras Design

### Community 176 - ".render_chat_drawer"
Cohesion: 0.26
Nodes (11): AgentEditor, location(), render_agents_section(), AgentEditCancelCallback, AgentMenuCallback, AgentOpenCallback, AnyElement, IntoElement (+3 more)

### Community 177 - "UiManager"
Cohesion: 0.31
Nodes (4): EventLoopProxy, Path, String, UiManager

### Community 178 - ".maybe_handle_palette_key"
Cohesion: 0.31
Nodes (6): GpuiShellRoot, App, Context, KeyDownEvent, Self, Window

### Community 179 - "WindowConfig"
Cohesion: 0.18
Nodes (13): kb(), KeyBind, build_leader_map(), build_leader_map_matches_default_keybinds_lua(), kb(), LeaderAction, FocusDir, HashMap (+5 more)

### Community 180 - "gpui M3c: Workspace Layer + Workspace Sidebar Drawer Implementation Plan"
Cohesion: 0.25
Nodes (7): Exit Criteria (from the M3 design's §7, the M3c slice of it), Global Constraints, gpui M3c: Workspace Layer + Workspace Sidebar Drawer Implementation Plan, Task 1: `WorkspaceManager` data model, Task 2: Wire `WorkspaceManager` into `GpuiShellRoot`, Task 3: Workspace CRUD keybinds (new / close / switch), Task 4: Workspace sidebar drawer (list, switch, create, close, rename)

### Community 181 - "gpui M3d: Sidebar MCP/Skills/Steering Sections + InfoOverlay Implementation Plan"
Cohesion: 0.25
Nodes (7): Exit Criteria, Global Constraints, gpui M3d: Sidebar MCP/Skills/Steering Sections + InfoOverlay Implementation Plan, Task 1: Wire `SkillManager`/`SteeringManager`/`McpManager` into `GpuiShellRoot`, Task 2: `InfoOverlay` — a scrollable, modal content popup, Task 3: Sidebar section-switching skeleton (focus model + Tab/arrow navigation), Task 4: MCP + Skills + Steering sections

### Community 182 - "list_git_branches_sync"
Cohesion: 0.28
Nodes (5): GpuiShellRoot, list_git_branches_sync(), Path, String, Vec

### Community 183 - "render_header"
Cohesion: 0.36
Nodes (7): header_status(), render_header(), ChatPanel, IntoElement, Option, String, short_model_name()

### Community 185 - ".begin_tab_rename"
Cohesion: 0.57
Nodes (4): GpuiShellRoot, Context, Self, Window

### Community 186 - "gpui M4a: Command Palette Implementation Plan"
Cohesion: 0.29
Nodes (6): Exit Criteria, Global Constraints, gpui M4a: Command Palette Implementation Plan, Task 1: Wire `CommandPalette` state + `Leader o` keybind (plumbing + event round-trip), Task 2: `palette.rs` -- render, focus guard, and an interim 3-action dispatch, Task 3: `palette_dispatch.rs` -- the full filtered action list + dispatch table

### Community 187 - "gpui M4b: Search Bar Implementation Plan"
Cohesion: 0.29
Nodes (6): Exit Criteria, Global Constraints, gpui M4b: Search Bar Implementation Plan, Task 1: Extract the search algorithm to `crate::term::search`; wire `SearchBar` state + `Cmd+F`, Task 2: Render the search bar; port the dirty/scroll_needed driver logic, Task 3: Highlight matches in the terminal grid

### Community 188 - "gpui M4c: Context Menu (Scoped Down) Implementation Plan"
Cohesion: 0.29
Nodes (6): Exit Criteria, Global Constraints, gpui M4c: Context Menu (Scoped Down) Implementation Plan, Task 1: `ContextMenu` state, dispatch logic, and the `Cmd+K` keybind, Task 2: Terminal grid right-click — Copy/Paste/Clear menu, Task 3: Tab-bar right-click — tab color picker

### Community 189 - "M5b — Chat Composer Extras Implementation Plan"
Cohesion: 0.29
Nodes (6): Exit Criteria, Global Constraints, M5b — Chat Composer Extras Implementation Plan, Task 1: `Leader a e` / `Leader a f` + palette entries, Task 2: Suggestion pills (zero-state + post-response), Task 3: File attachment picker

### Community 190 - "keybind_view.rs"
Cohesion: 0.18
Nodes (15): direct_bindings_view(), direct_bindings_view_filters_out_leader_case_insensitive(), direct_bindings_view_output_parses_into_real_actions(), DirectBindingsView, leader_bindings_view(), leader_bindings_view_carries_leader_key(), leader_bindings_view_filters_to_leader_mods_only_case_insensitive(), LeaderBindingsView (+7 more)

### Community 191 - ".handle_slash_command"
Cohesion: 0.33
Nodes (4): GpuiShellRoot, Context, Self, String

### Community 192 - ".show_toast"
Cohesion: 0.29
Nodes (6): GpuiShellRoot, Context, Duration, Into, Self, String

### Community 193 - "M4d — Toasts Implementation Plan"
Cohesion: 0.33
Nodes (5): Exit Criteria, Global Constraints, M4d — Toasts Implementation Plan, Task 1: Toast state, render, and poll-tick auto-expiry, Task 2: Wire config-reload as the toast's first real trigger

### Community 194 - ".spawn"
Cohesion: 0.07
Nodes (26): Dependencies & Execution Order, Format: `[ID] [P?] [Story] Description`, Implementation for User Story 1, Implementation for User Story 2, Implementation for User Story 3, Implementation Strategy, Incremental Delivery, MVP First (User Story 1 Only) (+18 more)

### Community 196 - ".write_key_to_terminal"
Cohesion: 0.40
Nodes (4): GpuiShellRoot, Context, KeyDownEvent, Self

### Community 197 - "cache.rs"
Cohesion: 0.10
Nodes (11): endpoint_id(), Error, hex(), random32(), Option, Result, String, unhex() (+3 more)

### Community 198 - ".new"
Cohesion: 0.13
Nodes (11): LcdAtlasEntry, LcdCacheEntry, LcdGlyphAtlas, HashMap, Option, Queue, Result, Sampler (+3 more)

### Community 199 - "draw_glyphs"
Cohesion: 0.32
Nodes (6): AnsiColor, Result, String, Vec, AtlasError, String

### Community 200 - "BatteryStatus"
Cohesion: 0.15
Nodes (11): mcp_overlay_content(), String, McpManager, HashMap, McpConfig, Option, Result, Self (+3 more)

### Community 208 - "AgentStepResult"
Cohesion: 0.18
Nodes (16): RemoteConfig, configure(), hostname(), lan_ip(), Manager, Arc, Mutex, Option (+8 more)

### Community 209 - ".rasterize_lcd_to_atlas"
Cohesion: 0.13
Nodes (14): AiPollResult, classify_llm_error(), Arc, EventLoopProxy, Receiver, RenderContext, Result, Runtime (+6 more)

### Community 211 - "cfdict_str"
Cohesion: 0.18
Nodes (19): fit_terminal(), PaneRenderCx, render_leaf(), render_pane_tree(), render_split(), Bounds, Div, HashMap (+11 more)

### Community 212 - "built_in_actions"
Cohesion: 0.09
Nodes (22): Architecture, Attention, Client, Consequences of the decisions, Decisions (2026-10-16), Goal, Host side, Implementation status (+14 more)

### Community 213 - "term/agents.rs"
Cohesion: 0.15
Nodes (18): default_agents(), detect(), merged_agents(), PaneProbe, parse_ps_rows(), RecognizedAgent, Self, Vec (+10 more)

### Community 214 - "cli.rs"
Cohesion: 0.10
Nodes (37): args(), deliver(), dry_run_and_invalid_json_do_not_write(), hook(), hooks(), install_claude_hooks(), install_creates_backs_up_and_is_idempotent(), notify() (+29 more)

### Community 215 - "rasterize_grid"
Cohesion: 0.32
Nodes (6): CachedFrame, evict_all(), evict_terminal(), App, Arc, RenderImage

### Community 216 - "RemoteOverlay"
Cohesion: 0.14
Nodes (14): GpuiShellRoot, qr_rows(), RemoteOverlay, render(), Context, Duration, Instant, IntoElement (+6 more)

### Community 217 - "term/attention.rs"
Cohesion: 0.22
Nodes (19): AgentState, Aggregate, aggregate_takes_highest_priority_and_any_unread(), AttentionEvent, bell_ignored_unless_enabled(), clear_keeps_working_state(), clear_resets_unread_and_state(), hook_needs_input_uses_message_and_respects_observed() (+11 more)

### Community 218 - "Packet"
Cohesion: 0.18
Nodes (12): ClientMsg, DataKind, direction_is_enforced(), HostMsg, Packet, PaneInfo, Option, PaneInfo (+4 more)

### Community 219 - "ipc/mod.rs"
Cohesion: 0.15
Nodes (20): Message, msg(), msg_requests_parse_and_hook_messages_are_unchanged(), MsgPayload, pane_env(), pane_env_exports_id_and_binary(), parse_line(), parse_line_rejects_garbage_and_accepts_unknown_fields() (+12 more)

### Community 220 - "control.rs"
Cohesion: 0.24
Nodes (18): apply(), cli(), command(), devices(), find_socket(), pair(), print_qr(), report() (+10 more)

### Community 221 - "GpuiShellRoot"
Cohesion: 0.18
Nodes (7): GpuiShellRoot, Context, HashMap, Option, Self, String, Vec

### Community 222 - "shell.rs"
Cohesion: 0.20
Nodes (16): bash_gets_shim_first_despite_user_rc(), ensure_zdotdir(), first_path_entry(), integration_env(), Option, Path, PathBuf, Result (+8 more)

### Community 223 - "app/renderer/mod.rs"
Cohesion: 0.62
Nodes (4): GpuiShellRoot, Context, Self, Window

### Community 224 - "AgentScanState"
Cohesion: 0.19
Nodes (10): AgentScanState, force_bypasses_interval_and_stuck_scan_is_replaced(), GpuiShellRoot, Instant, Option, String, Vec, run_ps() (+2 more)

### Community 225 - "PairingCode"
Cohesion: 0.17
Nodes (10): expiry(), PairingCode, rejects_bad_input(), round_trip(), Debug, Formatter, Result, Self (+2 more)

### Community 226 - "claude_hooks.rs"
Cohesion: 0.29
Nodes (3): Option, String, session_from_args()

### Community 227 - "bridge.rs"
Cohesion: 0.11
Nodes (22): Mux, Option, PaneInfo, Vec, GpuiPanes, HashMap, Option, PaneInfo (+14 more)

### Community 228 - "Tasks: Agents Sidebar Tab"
Cohesion: 0.10
Nodes (20): Amendment 2026-11-06: session id (US5) and custom names (US6), Amendment dependencies, Amendment implementation strategy, Conventions, Dependencies & Execution Order, Format: `[ID] [P?] [Story] Description`, Implementation Strategy, Parallel Opportunities (+12 more)

### Community 229 - "Tee"
Cohesion: 0.32
Nodes (9): dropped_receiver_is_removed(), AtomicUsize, Mutex, Receiver, Vec, send_without_subscribers_is_a_no_op(), slow_subscriber_is_dropped_without_affecting_others(), snapshot_first_then_output_in_order() (+1 more)

### Community 230 - "claude_shim.rs"
Cohesion: 0.25
Nodes (13): Env, injects_hooks_matching_the_rust_definition(), passes_through_outside_a_pane(), passes_through_with_user_settings_and_subcommands(), reports_missing_real_claude(), PathBuf, String, TempDir (+5 more)

### Community 231 - "Core Principles"
Cohesion: 0.15
Nodes (12): Core Principles, Development Workflow & Quality Gates, Governance, I. Preserve the Two-Binary, One-Core Architecture, II. Delegate, Do Not Reimplement, the Terminal Engine, III. Config Is Lua, Fully Re-evaluated, Atomically Swapped, IV. Threading and Async Discipline, PetruTerm Constitution (+4 more)

### Community 232 - "Feature Specification: [FEATURE NAME]"
Cohesion: 0.15
Nodes (12): Assumptions, Edge Cases, Feature Specification: [FEATURE NAME], Functional Requirements, Key Entities *(include if feature involves data)*, Measurable Outcomes, Requirements *(mandatory)*, Success Criteria *(mandatory)* (+4 more)

### Community 233 - "RecognizedAgent"
Cohesion: 0.31
Nodes (9): color_is_parsed_and_bad_color_is_ignored(), invalid_entries_are_skipped(), parse(), parse_entry(), parse_recognized(), LuaTable, Option, Vec (+1 more)

### Community 234 - "hook_cli.rs"
Cohesion: 0.29
Nodes (9): ExitStatus, first_line(), hook_sends_mapped_state_to_the_socket(), notify_sends_state_title_and_body(), outside_a_pane_or_with_a_dead_socket_hooks_exit_zero(), Path, UnixListener, Value (+1 more)

### Community 235 - "Core Principles"
Cohesion: 0.18
Nodes (10): Core Principles, Governance, [PRINCIPLE_1_NAME], [PRINCIPLE_2_NAME], [PRINCIPLE_3_NAME], [PRINCIPLE_4_NAME], [PRINCIPLE_5_NAME], [PROJECT_NAME] Constitution (+2 more)

### Community 236 - "Research: Agents Sidebar Tab"
Cohesion: 0.11
Nodes (17): Amendment 2026-11-06: session id and custom names, R10. Session id source for Claude Code, R11. Session id source for Kiro (OPEN: needs a verification spike), R12. Which source wins, and when an id is stale, R13. Custom name storage, validation, uniqueness, R14. Right-click menu and inline rename, R15. Process identity needs the agent pid, R1. How to find which panes run an agent (+9 more)

### Community 237 - "build_shaping_spans"
Cohesion: 0.22
Nodes (9): attrs_for(), Attrs, FontFeatures, build_shaping_spans(), Attrs, CellColorStyle, FontFeatures, String (+1 more)

### Community 238 - "Implementation Plan: [FEATURE]"
Cohesion: 0.22
Nodes (8): Complexity Tracking, Constitution Check, Documentation (this feature), Implementation Plan: [FEATURE], Project Structure, Source Code (repository root), Summary, Technical Context

### Community 239 - "Data Model: Agents Sidebar Tab"
Cohesion: 0.14
Nodes (13): AgentDetection (scan output), AgentRow (view model, built per render from detections + workspaces + tracker), Amendment 2026-11-06: session id and custom names, Changes to existing entities, Data Model: Agents Sidebar Tab, Name lookup (future bus, FR-027), New entities, PaneProbe (scan input) (+5 more)

### Community 240 - "GpuiPanes"
Cohesion: 0.19
Nodes (16): adjust_parent_split(), contains_leaf(), drag_separator_tracks_the_pointer_across_the_whole_split(), drag_separator_without_a_cached_rect_is_a_no_op(), drag_split_ratio(), PaneForest, RectCache, root_node_id() (+8 more)

### Community 241 - "String"
Cohesion: 0.17
Nodes (11): msg(), parse_reply(), reply_line(), request(), Path, String, send(), exit_codes_and_names_match_the_contract() (+3 more)

### Community 242 - "mux/remote.rs"
Cohesion: 0.33
Nodes (5): AgentRename, Entity, Option, String, TextInput

### Community 243 - "RowRevisionMap"
Cohesion: 0.16
Nodes (9): LcdCursorPatch, OverlayUploadPlan, production_cursor_builder_and_overlay_upload_state_are_connected(), RenderContext, RenderOverlayState, Mux, Option, PaneInfo (+1 more)

### Community 244 - "[CHECKLIST TYPE] Checklist: [FEATURE NAME]"
Cohesion: 0.40
Nodes (4): [Category 1], [Category 2], [CHECKLIST TYPE] Checklist: [FEATURE NAME], Notes

### Community 246 - ".pair_remote"
Cohesion: 0.70
Nodes (3): Context, Self, super::GpuiShellRoot

### Community 248 - "Quickstart: validating the Agents sidebar tab"
Cohesion: 0.40
Nodes (4): Amendment: session id and custom names (manual, gpui binary), Automated, Manual (gpui binary), Quickstart: validating the Agents sidebar tab

### Community 249 - ".route"
Cohesion: 0.24
Nodes (9): AgentMsgState, echo_name(), GpuiShellRoot, pid_alive(), HashMap, Instant, Option, String (+1 more)

### Community 250 - ".agent_probes"
Cohesion: 0.18
Nodes (19): AgentRow, basename(), format_age(), is_interpreter(), match_agent(), parse_hex_color(), parse_row(), ProcessRow (+11 more)

### Community 256 - "spawn_terminal_at"
Cohesion: 0.17
Nodes (14): GpuiShellRoot, Context, Self, GpuiShellRoot, Context, Self, Window, Arc (+6 more)

### Community 257 - "InfoOverlay"
Cohesion: 0.18
Nodes (9): GpuiShellRoot, InfoOverlay, Context, Default, KeyDownEvent, ScrollHandle, Self, String (+1 more)

### Community 258 - "claude_hooks.rs"
Cohesion: 0.20
Nodes (15): event_mapping(), fragment_registers_every_event(), group(), hook_command(), merge_into(), merge_preserves_user_settings_and_is_idempotent(), message_for(), notification_uses_payload_message_with_fallback() (+7 more)

### Community 259 - "Tasks: Agent-to-Agent Messaging"
Cohesion: 0.12
Nodes (15): Conventions, Dependencies & Execution Order, Format: `[ID] [P?] [Story] Description`, Implementation Strategy, Parallel Opportunities, Phase 1: Setup, Phase 2: Foundational (blocks every user story), Phase 3: User Story 1 - An agent sends a message to another by name (Priority: P1) MVP (+7 more)

### Community 260 - "Research: Agent-to-Agent Messaging"
Cohesion: 0.13
Nodes (14): Open items to verify during implementation, R10. Message text is never persisted, R11. Instruction files, R12. Wording of the delivered message, R1. Delivery mechanism (verified before planning), R2. Round trip needs an imperative reply instruction (verified), R3. Request/response over the control socket, R4. Sender identity (not spoofable from the command line) (+6 more)

### Community 261 - "AgentDetection"
Cohesion: 0.21
Nodes (7): AgentAccept, det(), replaced_or_gone_process_clears_acceptance(), HashMap, det(), name_is_dropped_when_the_process_goes_or_is_replaced(), AgentDetection

### Community 262 - "decide.rs"
Cohesion: 0.28
Nodes (10): decide(), DecideInput, each_refusal_has_its_outcome(), eleven_messages_in_a_minute_from_one_sender_are_rate_limited(), ok_input(), order_unknown_before_acceptance_before_limits(), Instant, Result (+2 more)

### Community 263 - "Pty"
Cohesion: 0.20
Nodes (6): pid_t, Pty, Drop, JoinHandle, Mutex, Receiver

### Community 264 - "Data Model: Agent-to-Agent Messaging"
Cohesion: 0.17
Nodes (11): AgentAccept, AgentRow additions (spec 001 view model), Compose editor state (gpui), Conversations (limits), Data Model: Agent-to-Agent Messaging, MsgRequest (socket, transient), Outcome (reply to the sender), RateLimiter (+3 more)

### Community 265 - "FreeTypeCmapLookup"
Cohesion: 0.20
Nodes (7): FreeTypeCmapLookup, Drop, FT_Face, FT_Library, Option, Path, Self

### Community 266 - "peer.rs"
Cohesion: 0.26
Nodes (8): a_connecting_child_is_identified_and_traced_back_to_us(), ancestors(), ancestors_of_a_child_include_this_process(), parent_of(), peer_pid(), Option, UnixStream, Vec

### Community 267 - "Option"
Cohesion: 0.31
Nodes (4): RenamePrompt, Key, Option, String

### Community 268 - "build_frame"
Cohesion: 0.38
Nodes (10): build_frame(), named(), named_agent_no_reply(), named_agent_with_reply(), named_head(), named_shell_can_be_answered(), String, unnamed_agent_and_shell_cannot_be_answered() (+2 more)

### Community 269 - "InfoOverlay"
Cohesion: 0.22
Nodes (4): InfoOverlay, Self, String, Vec

### Community 270 - "BatteryStatus"
Cohesion: 0.46
Nodes (7): c_void, BatteryStatus, cfdict_i32(), cfdict_str(), query(), Option, String

### Community 271 - ".begin_agent_compose"
Cohesion: 0.62
Nodes (4): GpuiShellRoot, Context, Self, Window

### Community 272 - "McpServerConfig"
Cohesion: 0.29
Nodes (7): McpServerConfig, HashMap, McpServer, String, Vec, to_acp_servers(), to_acp_servers_maps_stdio_shape()

### Community 273 - "run_io_loop"
Cohesion: 0.33
Nodes (6): BufWriter, ChildStdin, ChildStdout, Receiver, run_io_loop(), write_frame()

### Community 274 - "AgentCompose"
Cohesion: 0.33
Nodes (5): AgentCompose, Entity, Option, String, TextInput

### Community 275 - ".render_chat_drawer"
Cohesion: 0.33
Nodes (5): GpuiShellRoot, ChatPillCallback, Context, IntoElement, Self

### Community 277 - "Messages between agents in PetruTerm"
Cohesion: 0.40
Nodes (4): How to answer, Messages between agents in PetruTerm, Sending a message yourself, When a message arrives

### Community 278 - "Messages between agents in PetruTerm"
Cohesion: 0.40
Nodes (4): How to answer, Messages between agents in PetruTerm, Sending a message yourself, When a message arrives

### Community 279 - "validate_path"
Cohesion: 0.50
Nodes (4): Path, PathBuf, Result, validate_path()

### Community 280 - "Quickstart: validating agent-to-agent messaging"
Cohesion: 0.50
Nodes (3): Automated, Manual (gpui binary), Quickstart: validating agent-to-agent messaging

## Ambiguous Edges - Review These
- `Phase 9 UI Restyle Complete` → `Phase 4 Plugin Ecosystem Focus`  [AMBIGUOUS]
  .context/core/ACTIVE_CONTEXT.md · relation: conceptually_related_to

## Knowledge Gaps
- **520 isolated node(s):** `common.sh script`, `@modelcontextprotocol/server-filesystem`, `@modelcontextprotocol/server-fetch`, `build_pgo.sh script`, `CARGO_TARGET_DIR` (+515 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **20 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **What is the exact relationship between `Phase 9 UI Restyle Complete` and `Phase 4 Plugin Ecosystem Focus`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **Why does `Config` connect `Self` to `spawn_terminal_at`, `Src App`, `gpu.rs`, `Src Llm`, `Src Term`, `Src Renderer`, `Src Term`, `Src App`, `Src Config`, `Src App`, `spawn_acp_connect`, `Src Llm`, `Src App`, `snippets.rs`, `AppMenu`, `Src Llm`, `Src App`, `UiManager`, `WindowConfig`, `keybind_view.rs`, `Src App`, `AcpAgentConfig`, `cache.rs`, `draw_glyphs`, `Src Renderer`, `AgentStepResult`, `.rasterize_lcd_to_atlas`, `LcdGlyphAtlas`, `Mux`, `gpu.rs`, `RowRevisionMap`, `CommandPalette`, `build_font_system`?**
  _High betweenness centrality (0.176) - this node is a cross-community bridge._
- **Why does `GpuiShellRoot` connect `Src Llm` to `Self`, `AgentDetection`, `Src Term`, `Src Llm`, `Src Ui`, `Src Term`, `AgentCompose`, `Src Llm`, `Src Llm`, `Src Renderer`, `Src App`, `AppMenu`, `WakeupGate`, `Src Llm`, `status_bar/battery.rs`, `WindowConfig`, `Src App`, `BatteryStatus`, `Src App`, `RemoteOverlay`, `AgentScanState`, `CommandPalette`, `resolve_color`, `GpuiPanes`, `atlas.rs`, `mux/remote.rs`, `CellVertex`, `RenderContext`, `.route`?**
  _High betweenness centrality (0.142) - this node is a cross-community bridge._
- **Why does `ColorScheme` connect `Src App` to `Src Term`, `Src Llm`, `Self`, `Src Font`, `sections.rs`, `Src Config`, `.clear_color`, `Src Ui`, `Src App`, `Src Llm`, `Src App`, `Src App`, `.render_chat_drawer`, `render_header`, `WakeupGate`, `cache.rs`, `cfdict_str`, `RemoteOverlay`, `.handle_redraw`, `RenamePrompt`, `resolve_color`, `Pty`, `RowRevisionMap`, `AppMenu`, `CommandPalette`?**
  _High betweenness centrality (0.114) - this node is a cross-community bridge._
- **What connects `common.sh script`, `@modelcontextprotocol/server-filesystem`, `@modelcontextprotocol/server-fetch` to the rest of the system?**
  _520 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Src Config` be split into smaller, more focused modules?**
  _Cohesion score 0.0625 - nodes in this community are weakly interconnected._
- **Should `Src App` be split into smaller, more focused modules?**
  _Cohesion score 0.13015873015873017 - nodes in this community are weakly interconnected._