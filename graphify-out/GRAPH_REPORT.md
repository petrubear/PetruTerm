# Graph Report - PetruTerm  (2026-09-22)

## Corpus Check
- 238 files · ~402,362 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 3489 nodes · 6698 edges · 206 communities (197 shown, 9 thin omitted)
- Extraction: 98% EXTRACTED · 2% INFERRED · 0% AMBIGUOUS · INFERRED: 159 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `4d55293b`
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
- Config Default
- Src App
- Src Font
- LcdGlyphAtlas
- shaper.rs
- GRAPH-ARCH-01 First Slice Design
- Global Constraints
- Global Constraints
- .rasterize_lcd_to_atlas
- String
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
- UploadRange
- translate_key
- Global Constraints
- Global Constraints
- CellVertex
- RenderContext
- run_session
- AppMenu
- TermSize
- spawn_acp_connect
- Config
- .handle_redraw
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
- SearchBar
- snapshot.rs
- GpuiShellRoot
- build_shaping_spans
- M4 — Remaining Surfaces: Design
- build_frame_callbacks
- Rect
- .new
- workspace_snapshot.rs
- .open_terminal_for_acp
- M5a — ACP Agent Backend & Tool-Calling Design
- spawn_acp_connect
- M5c — Palette & Context-Menu Feature Completion Design
- M5d — Prompt Context Injection (Skills, Steering, Shell Context, MCP) Design
- .size
- .run_ai_query
- PaletteAction
- .handle_sidebar_focused_key
- snippets.rs
- GpuiShellRoot
- M5a — ACP Agent Backend & Tool-Calling Implementation Plan
- M5d — Prompt Context Injection Implementation Plan
- Action
- AppMenu
- draw_glyphs
- .maybe_handle_search_key
- status_bar/battery.rs
- full_grid_text
- M5c — Palette & Context-Menu Feature Completion Implementation Plan
- M5b — Chat Composer Extras Design
- .new
- UiManager
- .maybe_handle_palette_key
- .spawn
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
- String
- ChatPanelView
- .write_key_to_terminal
- .render_sidebar_drawer
- render_search_bar
- claude-stop-ci.sh

## God Nodes (most connected - your core abstractions)
1. `Config` - 94 edges
2. `ColorScheme` - 85 edges
3. `UiManager` - 64 edges
4. `FontConfig` - 54 edges
5. `Terminal` - 53 edges
6. `Mux` - 52 edges
7. `App` - 50 edges
8. `ChatPanel` - 50 edges
9. `GpuRenderer` - 50 edges
10. `RenderContext` - 41 edges

## Surprising Connections (you probably didn't know these)
- `Phase 4 Plugin Ecosystem Focus` --conceptually_related_to--> `Phase 9 UI Restyle Complete`  [AMBIGUOUS]
  AGENTS.md → .context/core/ACTIVE_CONTEXT.md
- `make_shaper()` --references--> `FontConfig`  [EXTRACTED]
  benches/build_instances.rs → src/config/schema.rs
- `make_shaper()` --references--> `TextShaper`  [EXTRACTED]
  benches/build_instances.rs → src/font/shaper.rs
- `build_row_vertices()` --references--> `FontConfig`  [EXTRACTED]
  benches/build_instances.rs → src/config/schema.rs
- `build_row_vertices()` --references--> `TextShaper`  [EXTRACTED]
  benches/build_instances.rs → src/font/shaper.rs

## Import Cycles
- 1-file cycle: `src/app/renderer/terminal.rs -> src/app/renderer/terminal.rs`
- 1-file cycle: `src/gpui_shell/status_bar/exit_code.rs -> src/gpui_shell/status_bar/exit_code.rs`
- 2-file cycle: `src/gpui_shell/rasterize/grid.rs -> src/term/mod.rs -> src/gpui_shell/rasterize/grid.rs`
- 2-file cycle: `src/font/freetype_lcd.rs -> src/renderer/lcd_atlas.rs -> src/font/freetype_lcd.rs`

## Hyperedges (group relationships)
- **Project Operational Context Set** — context_core_active_context_document, context_core_session_state_document, context_quality_technical_debt_document [INFERRED 0.85]
- **Planning and Specification Backbone** — context_specs_build_phases_document, context_specs_build_phases_archive_document, context_specs_term_specs_document [INFERRED 0.75]
- **Release Artifact Chain** — github_workflows_release_document, changelog_document, readme_document [INFERRED 0.65]

## Communities (206 total, 9 thin omitted)

### Community 1 - "Src App"
Cohesion: 0.12
Nodes (9): ChatPanel, ContextMenu, Instant, JoinHandle, Mux, PathBuf, Sender, Vec (+1 more)

### Community 2 - "Src Term"
Cohesion: 0.06
Nodes (37): command_end_deactivates(), command_start_deactivates(), ctrl_u_clears_before_cursor(), InputShadow, insert(), kill_word(), kill_word_at_start(), kill_word_trailing_spaces() (+29 more)

### Community 3 - "Src Llm"
Cohesion: 0.05
Nodes (45): GpuiShellRoot, InfoOverlay, Context, Default, KeyDownEvent, ScrollHandle, Self, String (+37 more)

### Community 4 - "Src Llm"
Cohesion: 0.08
Nodes (36): BufWriter, ChildStdin, ChildStdout, mcp_overlay_content(), String, dispatch_response(), extract_tool_result_text(), extract_tool_result_text_content_blocks() (+28 more)

### Community 5 - "Src Font"
Cohesion: 0.18
Nodes (13): AttrsList, build_attr_list(), is_pua(), Attrs, Device, FontSystem, PathBuf, Rc (+5 more)

### Community 6 - "Src Font"
Cohesion: 0.13
Nodes (12): LcdAtlasEntry, LcdCacheEntry, LcdGlyphAtlas, Device, HashMap, Option, Queue, Result (+4 more)

### Community 7 - "Src Ui"
Cohesion: 0.20
Nodes (8): drag_split_ratio(), find_rect(), FocusDir, PaneManager, PaneNode, remove_leaf(), Box, Option

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
Cohesion: 0.12
Nodes (14): AiPollResult, classify_llm_error(), Arc, EventLoopProxy, Receiver, RenderContext, Result, Runtime (+6 more)

### Community 12 - "Src Term"
Cohesion: 0.20
Nodes (16): EventListener, OnceLock, RawFd, pty_write_all(), PtyEvent, PtyEventProxy, reader_loop(), Arc (+8 more)

### Community 13 - "Src Renderer"
Cohesion: 0.09
Nodes (19): PresentMode, GpuRenderer, make_main_atlas_bind_group(), render_outcome_requires_rebuild(), RenderOutcome, Arc, BindGroup, Buffer (+11 more)

### Community 14 - "Src Llm"
Cohesion: 0.11
Nodes (26): agent_display_name(), Option, AcpAgentConfig, Vec, AcpSession, build_acp_agent(), PromptMsg, AcpAgent (+18 more)

### Community 16 - "Src Ui"
Cohesion: 0.09
Nodes (13): Option, String, SidebarState, label_format_and_truncation(), Default, Into, Option, Self (+5 more)

### Community 17 - "Src Term"
Cohesion: 0.06
Nodes (24): Cell, CursorShape, Dimensions, F, last_terminal_lines_for(), block_output_text_for(), GpuiShellRoot, row_text_and_absolute_row() (+16 more)

### Community 18 - "Src App"
Cohesion: 0.11
Nodes (20): ApplicationHandler, SeparatorDragState, App, Arc, Drop, EventLoopProxy, HashMap, InfoOverlay (+12 more)

### Community 19 - "Src Llm"
Cohesion: 0.14
Nodes (22): OnceCell, RequestBuilder, CachedJwt, CopilotProvider, CopilotTokenResponse, DeviceCodeResponse, keychain_load(), keychain_save() (+14 more)

### Community 20 - "Src Config"
Cohesion: 0.19
Nodes (25): LuaResult, LuaTable, bytecode_cache_path(), config_stdlib(), drain_lua_toast(), evict_stale_lua_cache(), fire_lua_event(), hash_path() (+17 more)

### Community 21 - "Src App"
Cohesion: 0.17
Nodes (11): File Map, Global Constraints, Task 1: Adding baseline performance counters, Task 2: Coalescing PTY wakeups and event-loop drains, Task 3: Propagating explicit dirty rows and revisions, Task 4: Storing terminal instances in stable row slots, Task 5: Adding merged GPU range uploads, Task 6: Proving equivalence and fallback behavior (+3 more)

### Community 22 - "Benches Build Instances.rs"
Cohesion: 0.07
Nodes (50): fit_terminal(), PaneRenderCx, render_leaf(), render_pane_tree(), render_split(), Bounds, Div, HashMap (+42 more)

### Community 23 - "Src App"
Cohesion: 0.10
Nodes (20): Event, FromStr, InputHandler, ActiveEventLoop, EventLoopProxy, HashMap, Instant, KeyEvent (+12 more)

### Community 24 - "Src App"
Cohesion: 0.22
Nodes (3): App, Option, Result

### Community 25 - "Src Ui"
Cohesion: 0.07
Nodes (28): ContextMenu, GpuiShellRoot, register_right_click(), render_context_menu(), Bounds, Context, ContextActionCallback, ContextMenuCloseCallback (+20 more)

### Community 26 - ".context Specs"
Cohesion: 0.11
Nodes (25): AGENTS Guide, Phase 4 Plugin Ecosystem Focus, Changelog, CLAUDE Instructions, System Map, Active Context, Phase 9 UI Restyle Complete, Session State (+17 more)

### Community 27 - "Src Llm"
Cohesion: 0.13
Nodes (8): App, Option, String, Option, PathBuf, Self, String, ShellContext

### Community 28 - "Src Ui"
Cohesion: 0.07
Nodes (40): Backspace, Copy, Cut, Delete, End, EntityInputHandler, Home, Left (+32 more)

### Community 29 - "Src Llm"
Cohesion: 0.11
Nodes (21): GpuiShellRoot, App, Arc, ChatPanelView, ContextMenu, Entity, Focusable, FocusHandle (+13 more)

### Community 30 - "Src Llm"
Cohesion: 0.18
Nodes (16): collect_skill_files(), extract_body(), extract_body_basic(), parse_frontmatter(), parse_frontmatter_basic(), parse_skill_file(), Option, Path (+8 more)

### Community 31 - "Src Renderer"
Cohesion: 0.07
Nodes (28): AnsiColor, Result, String, Vec, Queue, Result, AtlasEntry, AtlasError (+20 more)

### Community 32 - "Src Llm"
Cohesion: 0.10
Nodes (6): ActionPayload, AgentAction, parse_action_from_response(), Option, String, PanelState

### Community 33 - "Src Llm"
Cohesion: 0.22
Nodes (15): build_provider(), infer_context_window(), parse_agent_response(), parse_sse_chunk(), parse_usage(), Arc, Option, Result (+7 more)

### Community 34 - "Src Renderer"
Cohesion: 0.08
Nodes (26): brighten(), build_usage_hint(), calculate_row_hash(), colors_approx_eq(), GridVisualState, pack_color(), RenderContext, resolve_span_fg() (+18 more)

### Community 35 - "Src App"
Cohesion: 0.11
Nodes (18): NamedColor, resolve_line_fg(), RenderContext, ContextMenu, InfoOverlay, Option, StatusBar, ColorScheme (+10 more)

### Community 36 - "Src Llm"
Cohesion: 0.15
Nodes (15): AgentRequest, ApiMessage, build_api_messages(), ChatRequest, OpenAICompatProvider, ApiMessage, Client, Option (+7 more)

### Community 37 - "Src Llm"
Cohesion: 0.22
Nodes (8): AgentTool, execute_tool(), Option, Path, String, Value, Vec, ToolCall

### Community 38 - "Src App"
Cohesion: 0.18
Nodes (16): Flags, blend_pixel(), inverse_and_selection_compose(), inverse_flag_swaps_fg_bg(), plain_cell_resolves_fg_bg_unswapped(), resolve_cell_colors(), AnsiColor, FxHashMap (+8 more)

### Community 39 - "Src Llm"
Cohesion: 0.22
Nodes (11): command_returns_none_on_empty_panel(), command_returns_none_when_only_tool_lines(), command_strips_done_tool_line(), command_strips_in_progress_tool_line(), command_strips_markdown_fence_after_tool_lines(), command_strips_multiple_tool_lines(), command_without_tool_lines_unchanged(), panel_with_assistant() (+3 more)

### Community 40 - "Src Llm"
Cohesion: 0.15
Nodes (16): AgentRequest, ApiMessage, build_api_messages(), ChatRequest, keychain_api_key(), OpenRouterProvider, ApiMessage, Client (+8 more)

### Community 41 - "Src App"
Cohesion: 0.14
Nodes (19): App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId (+11 more)

### Community 42 - "Src Llm"
Cohesion: 0.07
Nodes (26): ai_block_body(), ai_block_hint(), AiBlockView, GpuiShellRoot, render_ai_block(), App, Arc, Context (+18 more)

### Community 43 - "Src App"
Cohesion: 0.29
Nodes (6): Global Constraints, GRAPH-ARCH-01 Remaining Domains Slice Implementation Plan, Self-Review, Task 1: Add `LeaderBindingsView` and migrate its two consumers, Task 2: Consolidate the duplicated scaled-font-with-LCD-fixup sequence, Task 3: Consolidate the duplicated `max_fps` interval formula

### Community 44 - "Src Llm"
Cohesion: 0.18
Nodes (5): MouseScrollDelta, PhysicalPosition, ActiveEventLoop, WindowEvent, WindowId

### Community 45 - "Src App"
Cohesion: 0.30
Nodes (7): PanelMsgParams, RenderContext, ChatPanel, String, dim(), idx_or_default(), T

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
Cohesion: 0.24
Nodes (11): ConfirmDisplay, Path, Self, Vec, compress_diff(), diff_lines(), DiffKind, DiffLine (+3 more)

### Community 50 - "Benches Rasterize.rs"
Cohesion: 0.14
Nodes (19): PaintQuad, PrepaintState, App, Bounds, Element, ElementId, Entity, GlobalElementId (+11 more)

### Community 51 - "Benches Search.rs"
Cohesion: 0.41
Nodes (12): bench_search_cold(), bench_search_cold_par(), bench_search_incremental(), build_flat_grid(), build_grid(), filter_matches(), push_search_match(), Criterion (+4 more)

### Community 53 - "Src Config"
Cohesion: 0.13
Nodes (30): Path, PathBuf, Result, validate_path(), env_vars_parsed(), load_from_paths(), load_global(), load_local() (+22 more)

### Community 54 - "Src App"
Cohesion: 0.24
Nodes (9): detects_absolute_path(), detects_stack_trace(), detects_url(), HoverLink, HoverLinkKind, is_boundary(), Option, String (+1 more)

### Community 55 - "Src Term"
Cohesion: 0.41
Nodes (12): cargo(), curl(), docker(), find(), git(), grep(), kubectl(), lookup_flag() (+4 more)

### Community 56 - "Src Renderer"
Cohesion: 0.32
Nodes (8): BindGroupLayout, CellPipeline, CellPipelineBgAware, CellPipelineLcd, Device, RenderPipeline, Self, TextureFormat

### Community 57 - "WakeupGate"
Cohesion: 0.18
Nodes (22): render_composer(), ChatPanelView, IntoElement, render_agent_action_card(), render_awaiting_confirm_card(), render_confirm_card(), render_diff_line(), IntoElement (+14 more)

### Community 58 - "Src Font"
Cohesion: 0.35
Nodes (6): FontLocator, FontPath, Default, Option, PathBuf, Self

### Community 59 - "Src Renderer"
Cohesion: 0.14
Nodes (10): RectUniforms, RoundedRectInstance, RoundedRectPipeline, BindGroup, Buffer, Device, Queue, RenderPipeline (+2 more)

### Community 60 - "Benches Shaping.rs"
Cohesion: 0.18
Nodes (22): Keystroke, alt_character_with_meta_prefixes_esc(), alt_character_without_meta_sends_composed_char(), arrow_up_app_cursor_mode_sends_ss3(), arrow_up_normal_mode_sends_csi(), backspace_sends_del(), ctrl_c_sends_control_byte(), ctrl_space_sends_nul() (+14 more)

### Community 61 - "ConfigWatcher"
Cohesion: 0.23
Nodes (9): RecommendedWatcher, ConfigWatcher, Duration, Option, Path, PathBuf, Receiver, Result (+1 more)

### Community 62 - "Src App"
Cohesion: 0.12
Nodes (20): closing_a_background_workspace_after_active_leaves_active_index_unchanged(), closing_a_background_workspace_before_active_keeps_the_same_workspace_active(), closing_an_unknown_workspace_id_is_refused(), closing_the_active_workspace_clamps_to_the_new_last_index(), closing_the_only_remaining_workspace_is_refused(), new_workspace_creates_and_activates_it(), next_and_prev_workspace_wrap_around(), rename_workspace_by_id_renames_a_non_active_workspace_and_leaves_active_alone() (+12 more)

### Community 63 - "Src Llm"
Cohesion: 0.44
Nodes (5): ChatMessage, ChatRole, Into, Self, String

### Community 64 - "Scripts Gen Icon.swift"
Cohesion: 0.31
Nodes (8): CGColor, CGFloat, CoreGraphics, Foundation, ImageIO, hex(), srgb(), UInt32

### Community 65 - "AcpAgentConfig"
Cohesion: 0.33
Nodes (5): llm_runtime_view(), llm_runtime_view_agent_path_requires_agent_config(), llm_runtime_view_preserves_backend_agent_and_ui_width(), llm_runtime_view_preserves_provider_defaults(), LlmRuntimeView

### Community 66 - ".handle_mouse_button"
Cohesion: 0.13
Nodes (15): closing_a_background_tab_after_active_leaves_active_index_unchanged(), closing_a_background_tab_before_active_keeps_the_same_tab_active(), closing_the_active_tab_still_clamps_to_the_new_last_index(), label_format_and_truncation(), rename_tab_by_id_renames_a_non_active_tab_and_leaves_active_alone(), rename_tab_with_unknown_id_returns_false_and_mutates_nothing(), Default, Into (+7 more)

### Community 67 - "Src Llm"
Cohesion: 0.39
Nodes (7): is_trusted(), Option, Path, PathBuf, Result, trust(), trust_file()

### Community 68 - "shaper.rs"
Cohesion: 0.08
Nodes (17): gpui_shell_actions(), Vec, render_command_palette(), Entity, IntoElement, TextInput, built_in_actions(), PaletteAction (+9 more)

### Community 69 - "Config Default"
Cohesion: 0.40
Nodes (5): fetch, filesystem, npx, @modelcontextprotocol/server-fetch, @modelcontextprotocol/server-filesystem

### Community 70 - "Src Term"
Cohesion: 0.15
Nodes (17): full_and_incremental_row_storage_are_equivalent(), layout_geometry_changes_require_rebuild(), lcd_slots_track_their_own_lengths(), row_slots_are_non_overlapping_and_bounded(), row_storage_equivalent(), RowSlot, RowWriteError, Option (+9 more)

### Community 71 - "Src Renderer"
Cohesion: 0.17
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

### Community 87 - "Src App"
Cohesion: 0.18
Nodes (7): LcdCursorPatch, OverlayUploadPlan, production_cursor_builder_and_overlay_upload_state_are_connected(), RenderContext, RenderOverlayState, Mux, Option

### Community 88 - "Src Font"
Cohesion: 0.08
Nodes (23): Architecture, Architecture decision: port the pane-tree algorithms, replace the rect math with taffy flex, Explicitly out of scope / not being built, File organization, gpui Chrome Migration — Design, Guiding principles, Input handling, Leader-key chorded dispatch (+15 more)

### Community 92 - "LcdGlyphAtlas"
Cohesion: 0.06
Nodes (35): App, blink_only_render(), blink_overlay_slot(), build_all_pane_instances(), production_overlay_upload_plan_keeps_cursor_first_and_terminal_ranges_separate(), production_upload_fallback_requests_full_rebuild_for_all_visible_rows(), ActiveEventLoop, Duration (+27 more)

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
Cohesion: 0.12
Nodes (4): Self, section_prev_cycles_backward_and_wraps(), SidebarSection, WorkspaceSidebar

### Community 98 - "String"
Cohesion: 0.22
Nodes (9): render_tab_bar(), AnyElement, Div, Option, Rgba, TabManager, TabRightClickCallback, tab_pill_tint() (+1 more)

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
Cohesion: 0.24
Nodes (9): context_block_empty(), context_block_with_files(), read_md_files(), Option, Path, Self, String, Vec (+1 more)

### Community 103 - "keybind_view.rs"
Cohesion: 0.12
Nodes (15): 1. `keybind_style` config property and schema, 2. One `keybinds.lua`, both tables, switch at the top, 3. Direct-bind resolution (`config::keybind_view`), 4. Dispatch wiring, per binary, 5. Proposed default "normal" style bindings, Current Problem, Data Flow, Dual Keybind Style Design (+7 more)

### Community 104 - ".on_key_down"
Cohesion: 0.20
Nodes (8): arrow_key_to_focus_dir(), GpuiShellRoot, Context, FocusDir, KeyDownEvent, Option, Self, Window

### Community 105 - ".handle_redraw"
Cohesion: 0.20
Nodes (7): FreeTypeCmapLookup, Drop, FT_Face, FT_Library, Option, Path, Self

### Community 106 - "RenamePrompt"
Cohesion: 0.36
Nodes (4): FT_Bitmap, Option, Queue, Vec

### Community 107 - "shaping.rs"
Cohesion: 0.53
Nodes (9): bench_shape_line_ascii(), bench_shape_line_ascii_cached(), bench_shape_line_ligatures(), bench_shape_line_ligatures_cached(), bench_shape_line_unicode(), make_colors(), make_shaper(), Criterion (+1 more)

### Community 108 - "Mux"
Cohesion: 0.15
Nodes (17): FnMut, SelectionRange, drain_pty_events(), eventloop_wakeup(), production_pty_drain_preserves_payload_and_special_events_across_budget(), PtyEventBatch, Arc, EventLoopProxy (+9 more)

### Community 109 - "gpu.rs"
Cohesion: 0.19
Nodes (11): LruCache, Metrics, CellStyle, has_ligature_chars(), Buffer, HashSet, String, SwashCache (+3 more)

### Community 110 - "resolve_color"
Cohesion: 0.05
Nodes (37): StatusBarColors, clears_cache_when_exit_code_returns_to_zero(), ExitCodeState, loads_nonzero_exit_code_and_skips_reload_when_mtime_unchanged(), Option, Path, PathBuf, SystemTime (+29 more)

### Community 111 - "run_session"
Cohesion: 0.20
Nodes (9): Global Constraints, M1b Exit Criteria, M1b (Grid Parity) Implementation Plan, Task 1: Split `font_state.rs` out of `terminal_element.rs`, Task 2: ANSI colors (fg/bg + bold/italic) + `rasterize.rs` extraction, Task 3: Cursor shapes + blink, Task 4: `mouse.rs` — click-drag selection, copy, click-to-focus, Task 5: Mouse-report passthrough (+1 more)

### Community 112 - "Pty"
Cohesion: 0.14
Nodes (26): FnOnce, collect_primary_face_ids(), compute_cell_size(), font_family(), font_features(), font_size(), FontState, header_row_min_height() (+18 more)

### Community 113 - "UploadRange"
Cohesion: 0.13
Nodes (9): bench_upload_bytes_comparison(), Result, validate_upload_range(), account_terminal_uploads(), merge_upload_ranges(), Vec, TerminalUploadAccounting, upload_ranges_bytes() (+1 more)

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
Cohesion: 0.18
Nodes (12): fetch_git_branch(), GitBranchState, poll_git_branch(), recover_stuck_in_flight(), Duration, Instant, Option, Path (+4 more)

### Community 118 - "RenderContext"
Cohesion: 0.33
Nodes (6): LayoutGlyph, glyph_to_cache_key(), CacheKey, ID, ShapedGlyph, should_use_lcd()

### Community 119 - "run_session"
Cohesion: 0.31
Nodes (8): AcpAgent, McpServer, PathBuf, Receiver, Result, Sender, Vec, run_session()

### Community 120 - "AppMenu"
Cohesion: 0.19
Nodes (12): HighlightStyle, char_range_to_byte_range(), heading_size(), render_line(), Div, Option, Pixels, span_highlight() (+4 more)

### Community 121 - "TermSize"
Cohesion: 0.25
Nodes (15): active_skill_continues_when_no_new_match(), attached_file_content_is_injected_with_header(), attached_file_over_cap_gets_truncated(), build_prompt_addendum(), empty_managers_produce_no_skill_or_steering_text(), matching_skill_gets_injected_and_recorded(), no_match_and_no_active_skill_leaves_matched_skill_none(), PromptAddendum (+7 more)

### Community 122 - "spawn_acp_connect"
Cohesion: 0.19
Nodes (13): to_u8_rgba(), fill_cell_backgrounds(), rasterize_grid(), Arc, CellColorStyle, Option, Pixels, Rc (+5 more)

### Community 123 - "Config"
Cohesion: 0.13
Nodes (10): home_str(), Mux, restore_pane_recursive(), RestorePaneContext, EventLoopProxy, Result, String, TabManager (+2 more)

### Community 124 - ".handle_redraw"
Cohesion: 0.36
Nodes (20): apply_row_offset(), bench_build_frame_dirty_rows(), bench_build_frame_hit(), bench_build_frame_hit_large_par(), bench_build_frame_hit_large_serial(), bench_build_frame_miss(), bench_build_row_hit(), bench_build_row_miss() (+12 more)

### Community 125 - ".dispatch_leader_action"
Cohesion: 0.29
Nodes (6): GpuiShellRoot, Context, IntoElement, Render, Self, Window

### Community 126 - "exit_code.rs"
Cohesion: 0.12
Nodes (15): 1. What the survey changed about the plan, 2. Slicing, 3.1 Text input: port gpui's own reference implementation, as an `Entity`, 3.2 Markdown: native gpui layout for messages, fixed-width for the input, 3.3 Layout: flex siblings, never a manual viewport rect, 3.4 Drawers, animated, 3.5 AI streaming: a channel owned by the shell root, drained in the poll loop, 3.6 Workspace layer mirrors `Mux` (+7 more)

### Community 127 - "build_font_system"
Cohesion: 0.08
Nodes (26): ChatPanelView, GpuiShellRoot, App, Arc, ChatPanel, Context, Entity, GpuiShellRoot (+18 more)

### Community 128 - "ExitCodeState"
Cohesion: 0.17
Nodes (11): Corrections made to the spec during planning, Dual Keybind Style (Data Layer + wgpu Binary) Implementation Plan, Follow-up (not in this plan), Global Constraints, Task 1: `KeybindStyle` config schema + Lua parsing + action whitelist, Task 2: `Mods` parser in `keybind_view.rs`, Task 3: `direct_bindings_view` in `keybind_view.rs`, Task 4: `keybinds.lua` — both tables, style switch, version bump (+3 more)

### Community 129 - "to_rgba"
Cohesion: 0.14
Nodes (17): App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId (+9 more)

### Community 130 - "gpu.rs"
Cohesion: 0.12
Nodes (5): Mux, FocusDir, HashMap, TabManager, WorkspaceData

### Community 131 - "render_line"
Cohesion: 0.31
Nodes (4): RenamePrompt, Key, Option, String

### Community 132 - "Self"
Cohesion: 0.10
Nodes (27): blur_translucency_only_when_translucent(), ChatUiConfig, Config, GpuPreference, KeybindStyle, KeyboardConfig, LeaderConfig, LlmBackend (+19 more)

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
Cohesion: 0.22
Nodes (6): pid_t, Pty, Drop, JoinHandle, Mutex, Receiver

### Community 137 - "ResizeHandleElement"
Cohesion: 0.16
Nodes (16): ResizeDragCallback, ResizeHandleElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId (+8 more)

### Community 138 - "pty_schedule.rs"
Cohesion: 0.27
Nodes (7): AtomicBool, gate_sends_once_until_drain(), Arc, Self, signal_during_drain_is_not_lost(), take_pending_clears_and_reports(), WakeupGate

### Community 139 - "to_rgba"
Cohesion: 0.19
Nodes (13): SectionSelectCallback, render_section_tabs(), render_workspace_sidebar(), AnyElement, Div, McpOpenCallback, Option, SkillOpenCallback (+5 more)

### Community 140 - "sections.rs"
Cohesion: 0.14
Nodes (21): render_browser_row(), render_empty_section(), render_mcp_section(), render_skills_section(), render_steering_section(), render_workspaces_section(), AnyElement, App (+13 more)

### Community 141 - "FreeTypeLcdRasterizer"
Cohesion: 0.15
Nodes (13): FreeTypeLcdRasterizer, LcdPixelMode, Device, Drop, FT_Face, FT_Library, HashMap, Mutex (+5 more)

### Community 142 - "Column"
Cohesion: 0.23
Nodes (10): Column, Line, cell_in_selection(), String, SelectionType, filter_matches(), push_search_match(), SearchMatch (+2 more)

### Community 143 - "leader.rs"
Cohesion: 0.16
Nodes (14): kb(), KeyBind, build_leader_map(), build_leader_map_matches_default_keybinds_lua(), kb(), LeaderAction, Error, FocusDir (+6 more)

### Community 144 - ".collect_grid_cells_for"
Cohesion: 0.27
Nodes (10): FlagHintOverlay, GhostOverlay, AnsiColor, FxHashMap, Option, SearchMatch, Vec, search_highlight_at() (+2 more)

### Community 145 - "spawn_terminal_at"
Cohesion: 0.20
Nodes (12): PaneForest, GpuiShellRoot, Context, Self, Window, Arc, Option, PathBuf (+4 more)

### Community 146 - "SearchBar"
Cohesion: 0.17
Nodes (6): Option, String, Vec, SearchBar, SearchMatch, truncated_count_label_shows_plus_suffix()

### Community 147 - "snapshot.rs"
Cohesion: 0.25
Nodes (15): list_saved_workspaces(), load_workspace(), PaneNodeSnapshot, Box, Option, PathBuf, Result, String (+7 more)

### Community 148 - "GpuiShellRoot"
Cohesion: 0.22
Nodes (5): GpuiShellRoot, App, Context, Self, SplitDir

### Community 149 - "build_shaping_spans"
Cohesion: 0.22
Nodes (9): attrs_for(), Attrs, FontFeatures, build_shaping_spans(), Attrs, CellColorStyle, FontFeatures, String (+1 more)

### Community 150 - "M4 — Remaining Surfaces: Design"
Cohesion: 0.14
Nodes (13): 1. Scope, 2. What's reusable vs. new, 3. M4a — Command Palette, 4.1 UI, 4.2 Search execution — the real decision, 4.3 Rendering matches, 4. M4b — Search Bar, 5. M4c — Context Menu (Scoped Down) (+5 more)

### Community 151 - "build_frame_callbacks"
Cohesion: 0.19
Nodes (13): FileDropCallback, build_file_drop_callback(), build_frame_callbacks(), build_tab_right_click_callback(), ChatPillCallback, Context, ContextActionCallback, ContextMenuCloseCallback (+5 more)

### Community 152 - "Rect"
Cohesion: 0.26
Nodes (13): adjust_parent_split(), collect_leaf_infos_impl(), collect_separators_impl(), contains_leaf(), next_node_id(), PaneInfo, PanePad, PaneSeparator (+5 more)

### Community 154 - "workspace_snapshot.rs"
Cohesion: 0.18
Nodes (7): next_node_id(), GpuiShellRoot, home_str(), Context, Result, Self, String

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
Cohesion: 0.47
Nodes (5): GpuiShellRoot, Context, Self, String, Window

### Community 162 - "PaletteAction"
Cohesion: 0.31
Nodes (3): ElementState, MouseButton, KeyEvent

### Community 163 - ".handle_sidebar_focused_key"
Cohesion: 0.35
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

### Community 168 - "Action"
Cohesion: 0.32
Nodes (6): CachedFrame, evict_all(), evict_terminal(), App, Arc, RenderImage

### Community 169 - "AppMenu"
Cohesion: 0.20
Nodes (8): Menu, MenuEvent, MenuId, AppMenu, Option, Self, Vec, Submenu

### Community 170 - "draw_glyphs"
Cohesion: 0.32
Nodes (7): draw_glyphs(), Buffer, CellColorStyle, FontSystem, RgbaImage, SwashCache, Vec

### Community 171 - ".maybe_handle_search_key"
Cohesion: 0.27
Nodes (6): GpuiShellRoot, App, Context, KeyDownEvent, Self, Window

### Community 172 - "status_bar/battery.rs"
Cohesion: 0.13
Nodes (18): c_void, BatterySaverMode, BatteryState, does_not_poll_before_ttl_expires(), poll_battery(), polls_again_once_ttl_expires(), resolve_battery_saver_active(), Duration (+10 more)

### Community 173 - "full_grid_text"
Cohesion: 0.24
Nodes (4): full_grid_text(), GpuiShellRoot, Option, String

### Community 174 - "M5c — Palette & Context-Menu Feature Completion Implementation Plan"
Cohesion: 0.22
Nodes (8): Exit Criteria, Global Constraints, M5c — Palette & Context-Menu Feature Completion Implementation Plan, Task 1: Command blocks, Task 2: Snippets, Task 3: Hover-link detection, Task 4: Git-branch picker, Task 5: Saved workspaces

### Community 175 - "M5b — Chat Composer Extras Design"
Cohesion: 0.22
Nodes (8): 1. Overview, 2. Global Constraints, 3. Task 1 -- `Leader a e` / `Leader a f` + palette entries, 4. Task 2 -- Suggestion pills (zero-state + post-response), 5. Task 3 -- File attachment picker, 6. Deferred (recorded, not reopened for reconsideration here), 7. Manual testing required (cannot be verified from the agent sandbox), M5b — Chat Composer Extras Design

### Community 176 - ".new"
Cohesion: 0.50
Nodes (3): GpuiShellRoot, Context, Self

### Community 177 - "UiManager"
Cohesion: 0.31
Nodes (4): EventLoopProxy, Path, String, UiManager

### Community 178 - ".maybe_handle_palette_key"
Cohesion: 0.31
Nodes (6): GpuiShellRoot, App, Context, KeyDownEvent, Self, Window

### Community 179 - ".spawn"
Cohesion: 0.39
Nodes (7): Child, open_pty(), Option, PathBuf, Result, Self, spawn_shell()

### Community 180 - "gpui M3c: Workspace Layer + Workspace Sidebar Drawer Implementation Plan"
Cohesion: 0.25
Nodes (7): Exit Criteria (from the M3 design's §7, the M3c slice of it), Global Constraints, gpui M3c: Workspace Layer + Workspace Sidebar Drawer Implementation Plan, Task 1: `WorkspaceManager` data model, Task 2: Wire `WorkspaceManager` into `GpuiShellRoot`, Task 3: Workspace CRUD keybinds (new / close / switch), Task 4: Workspace sidebar drawer (list, switch, create, close, rename)

### Community 181 - "gpui M3d: Sidebar MCP/Skills/Steering Sections + InfoOverlay Implementation Plan"
Cohesion: 0.25
Nodes (7): Exit Criteria, Global Constraints, gpui M3d: Sidebar MCP/Skills/Steering Sections + InfoOverlay Implementation Plan, Task 1: Wire `SkillManager`/`SteeringManager`/`McpManager` into `GpuiShellRoot`, Task 2: `InfoOverlay` — a scrollable, modal content popup, Task 3: Sidebar section-switching skeleton (focus model + Tab/arrow navigation), Task 4: MCP + Skills + Steering sections

### Community 182 - "list_git_branches_sync"
Cohesion: 0.29
Nodes (5): GpuiShellRoot, list_git_branches_sync(), Path, String, Vec

### Community 183 - "render_header"
Cohesion: 0.36
Nodes (7): header_status(), render_header(), ChatPanel, IntoElement, Option, String, short_model_name()

### Community 184 - ".dispatch_palette_action"
Cohesion: 0.25
Nodes (6): convert_focus_dir(), GpuiShellRoot, Context, FocusDir, Self, Window

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

### Community 194 - "String"
Cohesion: 0.27
Nodes (6): header_action_for_col(), header_action_label(), header_actions_start_col(), HeaderAction, Option, String

### Community 196 - ".write_key_to_terminal"
Cohesion: 0.40
Nodes (4): GpuiShellRoot, Context, KeyDownEvent, Self

### Community 198 - ".render_sidebar_drawer"
Cohesion: 0.40
Nodes (4): GpuiShellRoot, Context, IntoElement, Self

### Community 199 - "render_search_bar"
Cohesion: 0.50
Nodes (4): render_search_bar(), Entity, IntoElement, TextInput

## Ambiguous Edges - Review These
- `Phase 9 UI Restyle Complete` → `Phase 4 Plugin Ecosystem Focus`  [AMBIGUOUS]
  .context/core/ACTIVE_CONTEXT.md · relation: conceptually_related_to

## Knowledge Gaps
- **278 isolated node(s):** `@modelcontextprotocol/server-filesystem`, `@modelcontextprotocol/server-fetch`, `build_pgo.sh script`, `ci-local.sh script`, `RUSTFLAGS` (+273 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **9 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **What is the exact relationship between `Phase 9 UI Restyle Complete` and `Phase 4 Plugin Ecosystem Focus`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **Why does `Config` connect `Self` to `Src App`, `Src App`, `Src Term`, `Src Renderer`, `Src Llm`, `leader.rs`, `spawn_terminal_at`, `Src App`, `Src Term`, `Src Config`, `Src App`, `spawn_acp_connect`, `Src Llm`, `Src Renderer`, `Src Renderer`, `Src App`, `snippets.rs`, `Src Llm`, `status_bar/battery.rs`, `Src App`, `.new`, `UiManager`, `.spawn`, `.dispatch_palette_action`, `keybind_view.rs`, `AcpAgentConfig`, `shaper.rs`, `Src Renderer`, `Src App`, `LcdGlyphAtlas`, `Mux`, `Config`, `build_font_system`?**
  _High betweenness centrality (0.295) - this node is a cross-community bridge._
- **Why does `ColorScheme` connect `Src App` to `Src Config`, `Src Term`, `Self`, `to_rgba`, `sections.rs`, `Src Config`, `Benches Build Instances.rs`, `Src Ui`, `Src Renderer`, `Src App`, `Src App`, `Src Llm`, `Src App`, `render_header`, `WakeupGate`, `shaper.rs`, `render_search_bar`, `Src App`, `String`, `resolve_color`, `AppMenu`, `spawn_acp_connect`?**
  _High betweenness centrality (0.134) - this node is a cross-community bridge._
- **Why does `GpuiShellRoot` connect `Src Llm` to `Self`, `Src Llm`, `Src Term`, `pty_schedule.rs`, `leader.rs`, `Src Term`, `SearchBar`, `Benches Build Instances.rs`, `Src App`, `Src Llm`, `Src Llm`, `Src Llm`, `status_bar/battery.rs`, `Src App`, `shaper.rs`, `.rasterize_lcd_to_atlas`, `cfdict_str`, `resolve_color`, `CellVertex`?**
  _High betweenness centrality (0.084) - this node is a cross-community bridge._
- **What connects `@modelcontextprotocol/server-filesystem`, `@modelcontextprotocol/server-fetch`, `build_pgo.sh script` to the rest of the system?**
  _278 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Src App` be split into smaller, more focused modules?**
  _Cohesion score 0.12162162162162163 - nodes in this community are weakly interconnected._
- **Should `Src Term` be split into smaller, more focused modules?**
  _Cohesion score 0.06240084611316764 - nodes in this community are weakly interconnected._