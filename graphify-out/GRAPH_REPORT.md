# Graph Report - gpui-migration  (2026-09-04)

## Corpus Check
- 150 files · ~243,427 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 2411 nodes · 4937 edges · 120 communities (114 shown, 6 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 69 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `f9ea9f1c`
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
- Src App
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
- .resumed
- .handle_redraw
- .build_instances
- app/ui/mod.rs
- Mux
- gpu.rs
- resolve_color
- run_session
- RenamePrompt
- UploadRange
- translate_key
- Global Constraints
- Global Constraints
- CellVertex
- RenderContext
- run_session

## God Nodes (most connected - your core abstractions)
1. `Config` - 78 edges
2. `UiManager` - 64 edges
3. `FontConfig` - 54 edges
4. `Mux` - 52 edges
5. `App` - 50 edges
6. `ChatPanel` - 50 edges
7. `GpuRenderer` - 50 edges
8. `ColorScheme` - 42 edges
9. `RenderContext` - 41 edges
10. `Terminal` - 40 edges

## Surprising Connections (you probably didn't know these)
- `bench_upload_bytes_comparison()` --calls--> `account_terminal_uploads()`  [INFERRED]
  benches/build_instances.rs → src/renderer/upload.rs
- `bench_upload_bytes_comparison()` --calls--> `merge_upload_ranges()`  [INFERRED]
  benches/build_instances.rs → src/renderer/upload.rs
- `Phase 4 Plugin Ecosystem Focus` --conceptually_related_to--> `Phase 9 UI Restyle Complete`  [AMBIGUOUS]
  AGENTS.md → .context/core/ACTIVE_CONTEXT.md
- `make_shaper()` --references--> `FontConfig`  [EXTRACTED]
  benches/build_instances.rs → src/config/schema.rs
- `make_shaper()` --references--> `TextShaper`  [EXTRACTED]
  benches/build_instances.rs → src/font/shaper.rs

## Import Cycles
- 1-file cycle: `src/app/renderer/terminal.rs -> src/app/renderer/terminal.rs`
- 1-file cycle: `src/platform/battery.rs -> src/platform/battery.rs`
- 2-file cycle: `src/font/freetype_lcd.rs -> src/renderer/lcd_atlas.rs -> src/font/freetype_lcd.rs`

## Hyperedges (group relationships)
- **Project Operational Context Set** — context_core_active_context_document, context_core_session_state_document, context_quality_technical_debt_document [INFERRED 0.85]
- **Planning and Specification Backbone** — context_specs_build_phases_document, context_specs_build_phases_archive_document, context_specs_term_specs_document [INFERRED 0.75]
- **Release Artifact Chain** — github_workflows_release_document, changelog_document, readme_document [INFERRED 0.65]

## Communities (120 total, 6 thin omitted)

### Community 0 - "Src Config"
Cohesion: 0.11
Nodes (26): BatterySaverMode, blur_translucency_only_when_translucent(), ChatUiConfig, Config, GpuPreference, KeyboardConfig, LeaderConfig, LlmBackend (+18 more)

### Community 1 - "Src App"
Cohesion: 0.12
Nodes (9): ChatPanel, EventLoopProxy, Instant, JoinHandle, Mux, PathBuf, Sender, Vec (+1 more)

### Community 2 - "Src Term"
Cohesion: 0.06
Nodes (37): Range, command_end_deactivates(), command_start_deactivates(), ctrl_u_clears_before_cursor(), InputShadow, insert(), kill_word(), kill_word_at_start() (+29 more)

### Community 3 - "Src Llm"
Cohesion: 0.07
Nodes (36): char_chunks(), ChatPanel, Path, PathBuf, String, Vec, scan_dir(), scan_files() (+28 more)

### Community 4 - "Src Llm"
Cohesion: 0.08
Nodes (34): BufWriter, ChildStdin, ChildStdout, dispatch_response(), extract_tool_result_text(), extract_tool_result_text_content_blocks(), extract_tool_result_text_fallback_to_json(), McpClient (+26 more)

### Community 5 - "Src Font"
Cohesion: 0.06
Nodes (54): AttrsList, FnOnce, LayoutGlyph, LruCache, Metrics, build_attr_list(), CellStyle, FreeTypeCmapLookup (+46 more)

### Community 6 - "Src Font"
Cohesion: 0.07
Nodes (29): FT_Bitmap, FreeTypeLcdRasterizer, LcdAtlasEntry, LcdPixelMode, Device, Drop, FT_Face, FT_Library (+21 more)

### Community 7 - "Src Ui"
Cohesion: 0.06
Nodes (45): list_saved_workspaces(), load_workspace(), PaneNodeSnapshot, Box, Option, PathBuf, Result, String (+37 more)

### Community 8 - "Src Term"
Cohesion: 0.10
Nodes (24): Block, block_count_is_capped(), BlockManager, blocks_in_viewport_filters_correctly(), complete_block_lifecycle(), exit_code_nonzero(), incomplete_block_not_in_viewport(), mgr() (+16 more)

### Community 9 - "Src App"
Cohesion: 0.07
Nodes (50): Point, a_pane_that_never_started_the_gesture_is_not_marked_dragged(), a_pane_whose_gesture_already_ended_is_not_marked_dragged_by_a_later_pass_through(), accumulate_scroll_lines(), at_bottom_thumb_sits_at_bottom(), at_top_thumb_sits_at_top(), click_bottom_of_strip_targets_live_bottom(), click_count_caps_at_three() (+42 more)

### Community 10 - "Src Ui"
Cohesion: 0.12
Nodes (16): 1. Instrument the existing path, 2. Coalesce event-loop work, 3. Propagate row revisions, 4. Upload changed ranges, Current Problem, Data Flow, Error Handling and Safety, Execution Tasks (+8 more)

### Community 11 - "Src App"
Cohesion: 0.07
Nodes (23): App, blink_only_render(), blink_overlay_slot(), build_all_pane_instances(), production_overlay_upload_plan_keeps_cursor_first_and_terminal_ranges_separate(), production_upload_fallback_requests_full_rebuild_for_all_visible_rows(), ActiveEventLoop, Duration (+15 more)

### Community 12 - "Src Term"
Cohesion: 0.15
Nodes (25): Child, Event, EventListener, Fn, OnceLock, RawFd, open_pty(), pty_write_all() (+17 more)

### Community 13 - "Src Renderer"
Cohesion: 0.11
Nodes (12): PresentMode, GpuRenderer, Buffer, Color, Device, Option, Queue, Rc (+4 more)

### Community 14 - "Src Llm"
Cohesion: 0.13
Nodes (22): AcpAgentConfig, Vec, AcpSession, build_acp_agent(), PromptMsg, AcpAgent, Instant, JoinHandle (+14 more)

### Community 16 - "Src Ui"
Cohesion: 0.13
Nodes (10): label_format_and_truncation(), Default, Into, Option, Self, String, Vec, Tab (+2 more)

### Community 17 - "Src Term"
Cohesion: 0.08
Nodes (18): CursorShape, Dimensions, F, Rc, CursorInfo, process_cwd(), Arc, FairMutex (+10 more)

### Community 18 - "Src App"
Cohesion: 0.14
Nodes (17): ApplicationHandler, App, Arc, Drop, EventLoopProxy, HashMap, Instant, Lua (+9 more)

### Community 19 - "Src Llm"
Cohesion: 0.15
Nodes (22): OnceCell, RequestBuilder, CachedJwt, CopilotProvider, CopilotTokenResponse, DeviceCodeResponse, keychain_load(), keychain_save() (+14 more)

### Community 20 - "Src Config"
Cohesion: 0.18
Nodes (25): LuaResult, LuaTable, bytecode_cache_path(), config_stdlib(), drain_lua_toast(), evict_stale_lua_cache(), fire_lua_event(), hash_path() (+17 more)

### Community 21 - "Src App"
Cohesion: 0.17
Nodes (11): File Map, Global Constraints, Task 1: Adding baseline performance counters, Task 2: Coalescing PTY wakeups and event-loop drains, Task 3: Propagating explicit dirty rows and revisions, Task 4: Storing terminal instances in stable row slots, Task 5: Adding merged GPU range uploads, Task 6: Proving equivalence and fallback behavior (+3 more)

### Community 22 - "Benches Build Instances.rs"
Cohesion: 0.13
Nodes (29): adjust_parent_split(), close_focused_promotes_the_sibling(), close_focused_refuses_to_close_the_last_pane(), close_specific_moves_focus_off_the_closed_pane(), contains_leaf(), drag_separator_tracks_the_pointer_across_the_whole_split(), drag_separator_without_a_cached_rect_is_a_no_op(), drag_split_ratio() (+21 more)

### Community 23 - "Src App"
Cohesion: 0.15
Nodes (14): InputHandler, ActiveEventLoop, EventLoopProxy, HashMap, Instant, KeyEvent, Modifiers, Mux (+6 more)

### Community 24 - "Src App"
Cohesion: 0.22
Nodes (3): App, Option, Result

### Community 25 - "Src Ui"
Cohesion: 0.12
Nodes (16): ContextAction, ContextMenu, ContextMenuItem, default_items(), item(), item_kb(), label_item(), open_default_resets_tab_color_picker_items() (+8 more)

### Community 26 - ".context Specs"
Cohesion: 0.11
Nodes (25): AGENTS Guide, Phase 4 Plugin Ecosystem Focus, Changelog, CLAUDE Instructions, System Map, Active Context, Phase 9 UI Restyle Complete, Session State (+17 more)

### Community 27 - "Src Llm"
Cohesion: 0.13
Nodes (8): App, Option, String, Option, PathBuf, Self, String, ShellContext

### Community 28 - "Src Ui"
Cohesion: 0.15
Nodes (12): StatusBarColors, format_time(), Default, Option, Path, Self, String, Vec (+4 more)

### Community 29 - "Src Llm"
Cohesion: 0.27
Nodes (7): AtomicBool, gate_sends_once_until_drain(), Arc, Self, signal_during_drain_is_not_lost(), take_pending_clears_and_reports(), WakeupGate

### Community 30 - "Src Llm"
Cohesion: 0.18
Nodes (16): collect_skill_files(), extract_body(), extract_body_basic(), parse_frontmatter(), parse_frontmatter_basic(), parse_skill_file(), Option, Path (+8 more)

### Community 31 - "Src Renderer"
Cohesion: 0.08
Nodes (24): Queue, Result, AtlasEntry, AtlasError, ColorAtlas, dummy_entry(), dummy_key(), evict_cold_keeps_all_when_all_warm() (+16 more)

### Community 32 - "Src Llm"
Cohesion: 0.10
Nodes (6): ActionPayload, AgentAction, parse_action_from_response(), Option, String, PanelState

### Community 33 - "Src Llm"
Cohesion: 0.17
Nodes (17): build_provider(), infer_context_window(), LlmProvider, parse_agent_response(), parse_sse_chunk(), parse_usage(), Arc, Option (+9 more)

### Community 34 - "Src Renderer"
Cohesion: 0.29
Nodes (7): build_font_system(), locate_font_for_lcd(), FontSystem, ID, PathBuf, Result, String

### Community 35 - "Src App"
Cohesion: 0.14
Nodes (10): resolve_line_fg(), SidebarDrawParams, RenderContext, Option, Mux, ColorScheme, FontConfig, Color (+2 more)

### Community 36 - "Src Llm"
Cohesion: 0.15
Nodes (15): AgentRequest, ApiMessage, build_api_messages(), ChatRequest, OpenAICompatProvider, ApiMessage, Client, Option (+7 more)

### Community 37 - "Src Llm"
Cohesion: 0.22
Nodes (9): AgentStepResult, AgentTool, execute_tool(), Option, Path, String, Value, Vec (+1 more)

### Community 38 - "Src App"
Cohesion: 0.12
Nodes (26): Flags, FontFeatures, RenderImage, RgbaImage, attrs_for(), blend_pixel(), CachedFrame, evict_all() (+18 more)

### Community 39 - "Src Llm"
Cohesion: 0.18
Nodes (16): command_returns_none_on_empty_panel(), command_returns_none_when_only_tool_lines(), command_strips_done_tool_line(), command_strips_in_progress_tool_line(), command_strips_markdown_fence_after_tool_lines(), command_strips_multiple_tool_lines(), command_without_tool_lines_unchanged(), header_action_for_col() (+8 more)

### Community 40 - "Src Llm"
Cohesion: 0.15
Nodes (16): AgentRequest, ApiMessage, build_api_messages(), ChatRequest, keychain_api_key(), OpenRouterProvider, ApiMessage, Client (+8 more)

### Community 41 - "Src App"
Cohesion: 0.11
Nodes (30): PaneFocusCallback, Rgba, SeparatorDragCallback, fit_terminal(), PaneRenderCx, render_leaf(), render_pane_tree(), render_split() (+22 more)

### Community 42 - "Src Llm"
Cohesion: 0.14
Nodes (5): AiBlock, AiState, Option, Self, String

### Community 43 - "Src App"
Cohesion: 0.29
Nodes (6): Global Constraints, GRAPH-ARCH-01 Remaining Domains Slice Implementation Plan, Self-Review, Task 1: Add `LeaderBindingsView` and migrate its two consumers, Task 2: Consolidate the duplicated scaled-font-with-LCD-fixup sequence, Task 3: Consolidate the duplicated `max_fps` interval formula

### Community 44 - "Src Llm"
Cohesion: 0.22
Nodes (5): ElementState, MouseButton, SeparatorDragState, KeyEvent, Option

### Community 45 - "Src App"
Cohesion: 0.30
Nodes (7): PanelMsgParams, RenderContext, ChatPanel, String, dim(), idx_or_default(), T

### Community 46 - "Src App"
Cohesion: 0.36
Nodes (12): bench_rasterize_glyph_ascii(), bench_rasterize_line_ascii(), bench_rasterize_line_ligatures(), bench_rasterize_line_unicode(), make_colors(), make_shaper(), rasterize_one(), CacheKey (+4 more)

### Community 47 - "Src App"
Cohesion: 0.05
Nodes (46): Context, Focusable, FocusHandle, KeyDownEvent, Render, main(), kb(), leader_bindings_view() (+38 more)

### Community 48 - "Src App"
Cohesion: 0.21
Nodes (8): fetch_git_branch(), list_git_branches_sync(), Duration, Option, Path, String, Vec, UiManager

### Community 49 - "Src Llm"
Cohesion: 0.24
Nodes (11): ConfirmDisplay, Path, Self, Vec, compress_diff(), diff_lines(), DiffKind, DiffLine (+3 more)

### Community 50 - "Benches Rasterize.rs"
Cohesion: 0.17
Nodes (16): App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId (+8 more)

### Community 51 - "Benches Search.rs"
Cohesion: 0.41
Nodes (12): bench_search_cold(), bench_search_cold_par(), bench_search_incremental(), build_flat_grid(), build_grid(), filter_matches(), push_search_match(), Criterion (+4 more)

### Community 53 - "Src Config"
Cohesion: 0.16
Nodes (23): Path, PathBuf, Result, validate_path(), env_vars_parsed(), load_from_paths(), load_global(), load_local() (+15 more)

### Community 54 - "Src App"
Cohesion: 0.24
Nodes (9): detects_absolute_path(), detects_stack_trace(), detects_url(), HoverLink, HoverLinkKind, is_boundary(), Option, String (+1 more)

### Community 55 - "Src Term"
Cohesion: 0.41
Nodes (12): cargo(), curl(), docker(), find(), git(), grep(), kubectl(), lookup_flag() (+4 more)

### Community 56 - "Src Renderer"
Cohesion: 0.32
Nodes (8): BindGroupLayout, CellPipeline, CellPipelineBgAware, CellPipelineLcd, Device, RenderPipeline, Self, TextureFormat

### Community 57 - "Src App"
Cohesion: 0.10
Nodes (14): GridVisualState, RenderContext, RowCache, RowCacheEntry, Arc, Color, HashMap, HashSet (+6 more)

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
Cohesion: 0.34
Nodes (21): apply_row_offset(), bench_build_frame_dirty_rows(), bench_build_frame_hit(), bench_build_frame_hit_large_par(), bench_build_frame_hit_large_serial(), bench_build_frame_miss(), bench_build_row_hit(), bench_build_row_miss() (+13 more)

### Community 63 - "Src Llm"
Cohesion: 0.44
Nodes (5): ChatMessage, ChatRole, Into, Self, String

### Community 64 - "Scripts Gen Icon.swift"
Cohesion: 0.31
Nodes (8): CGColor, CGFloat, CoreGraphics, Foundation, ImageIO, hex(), srgb(), UInt32

### Community 65 - "AcpAgentConfig"
Cohesion: 0.27
Nodes (7): agent_display_name(), llm_runtime_view(), llm_runtime_view_agent_path_requires_agent_config(), llm_runtime_view_preserves_backend_agent_and_ui_width(), llm_runtime_view_preserves_provider_defaults(), LlmRuntimeView, Option

### Community 66 - ".handle_mouse_button"
Cohesion: 0.18
Nodes (5): MouseScrollDelta, PhysicalPosition, ActiveEventLoop, WindowEvent, WindowId

### Community 67 - "Src Llm"
Cohesion: 0.39
Nodes (7): is_trusted(), Option, Path, PathBuf, Result, trust(), trust_file()

### Community 68 - "shaper.rs"
Cohesion: 0.09
Nodes (16): FromStr, Action, built_in_actions(), PaletteAction, FocusDir, Option, Result, Self (+8 more)

### Community 69 - "Config Default"
Cohesion: 0.40
Nodes (5): fetch, filesystem, npx, @modelcontextprotocol/server-fetch, @modelcontextprotocol/server-filesystem

### Community 70 - "Src Term"
Cohesion: 0.19
Nodes (14): full_and_incremental_row_storage_are_equivalent(), layout_geometry_changes_require_rebuild(), lcd_slots_track_their_own_lengths(), row_slots_are_non_overlapping_and_bounded(), row_storage_equivalent(), RowSlot, RowWriteError, Option (+6 more)

### Community 71 - "Src Renderer"
Cohesion: 0.22
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
Cohesion: 0.22
Nodes (5): pid_t, Pty, Drop, JoinHandle, Receiver

### Community 88 - "Src Font"
Cohesion: 0.08
Nodes (23): Architecture, Architecture decision: port the pane-tree algorithms, replace the rect math with taffy flex, Explicitly out of scope / not being built, File organization, gpui Chrome Migration — Design, Guiding principles, Input handling, Leader-key chorded dispatch (+15 more)

### Community 92 - "LcdGlyphAtlas"
Cohesion: 0.11
Nodes (18): BuildDamage, capacity_overflow_is_deferred_to_the_originating_terminal(), deferred_terminal_rebuilds_are_removed_when_state_is_cleared(), DirtyRows, full_damage_covers_requested_rows(), FullRebuildTrigger, pending_full_rebuild_applies_to_every_terminal_built_in_the_frame(), production_build_contract_consumes_every_full_rebuild_trigger() (+10 more)

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
Cohesion: 0.24
Nodes (9): context_block_empty(), context_block_with_files(), read_md_files(), Option, Path, Self, String, Vec (+1 more)

### Community 98 - "String"
Cohesion: 0.12
Nodes (16): closing_a_background_tab_after_active_leaves_active_index_unchanged(), closing_a_background_tab_before_active_keeps_the_same_tab_active(), closing_the_active_tab_still_clamps_to_the_new_last_index(), label_format_and_truncation(), render_tab_bar(), Default, Div, Into (+8 more)

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
Cohesion: 0.46
Nodes (7): c_void, BatteryStatus, cfdict_i32(), cfdict_str(), query(), Option, String

### Community 103 - "keybind_view.rs"
Cohesion: 0.21
Nodes (7): make_main_atlas_bind_group(), render_outcome_requires_rebuild(), RenderOutcome, Arc, BindGroup, Self, Window

### Community 104 - ".resumed"
Cohesion: 0.53
Nodes (9): bench_shape_line_ascii(), bench_shape_line_ascii_cached(), bench_shape_line_ligatures(), bench_shape_line_ligatures_cached(), bench_shape_line_unicode(), make_colors(), make_shaper(), Criterion (+1 more)

### Community 105 - ".handle_redraw"
Cohesion: 0.31
Nodes (4): EventLoopProxy, Path, String, UiManager

### Community 106 - ".build_instances"
Cohesion: 0.20
Nodes (8): Menu, MenuEvent, MenuId, AppMenu, Option, Self, Vec, Submenu

### Community 107 - "app/ui/mod.rs"
Cohesion: 0.12
Nodes (13): Runtime, AiPollResult, classify_llm_error(), Arc, Receiver, RenderContext, Result, Self (+5 more)

### Community 108 - "Mux"
Cohesion: 0.07
Nodes (39): Column, FnMut, FxHashMap, Line, SearchMatch, SelectionRange, cell_in_selection(), drain_pty_events() (+31 more)

### Community 109 - "gpu.rs"
Cohesion: 0.29
Nodes (10): brighten(), build_usage_hint(), calculate_row_hash(), colors_approx_eq(), pack_color(), resolve_span_fg(), ChatPanel, String (+2 more)

### Community 110 - "resolve_color"
Cohesion: 0.21
Nodes (10): NamedColor, AnsiColor, Result, String, Vec, dim(), resolve_color(), resolve_indexed() (+2 more)

### Community 111 - "run_session"
Cohesion: 0.20
Nodes (9): Global Constraints, M1b Exit Criteria, M1b (Grid Parity) Implementation Plan, Task 1: Split `font_state.rs` out of `terminal_element.rs`, Task 2: ANSI colors (fg/bg + bold/italic) + `rasterize.rs` extraction, Task 3: Cursor shapes + blink, Task 4: `mouse.rs` — click-drag selection, copy, click-to-focus, Task 5: Mouse-report passthrough (+1 more)

### Community 112 - "RenamePrompt"
Cohesion: 0.31
Nodes (4): RenamePrompt, Key, Option, String

### Community 113 - "UploadRange"
Cohesion: 0.13
Nodes (11): CellUniforms, CellVertex, Result, validate_upload_range(), account_terminal_uploads(), merge_upload_ranges(), Vec, TerminalUploadAccounting (+3 more)

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
Cohesion: 0.21
Nodes (7): Cell, LcdCursorPatch, OverlayUploadPlan, production_cursor_builder_and_overlay_upload_state_are_connected(), RenderContext, RenderOverlayState, Option

### Community 119 - "run_session"
Cohesion: 0.43
Nodes (6): AcpAgent, PathBuf, Receiver, Result, Sender, run_session()

## Ambiguous Edges - Review These
- `Phase 9 UI Restyle Complete` → `Phase 4 Plugin Ecosystem Focus`  [AMBIGUOUS]
  .context/core/ACTIVE_CONTEXT.md · relation: conceptually_related_to

## Knowledge Gaps
- **115 isolated node(s):** `@modelcontextprotocol/server-filesystem`, `@modelcontextprotocol/server-fetch`, `build_pgo.sh script`, `bundle.sh script`, `ci-local.sh script` (+110 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **6 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **What is the exact relationship between `Phase 9 UI Restyle Complete` and `Phase 4 Plugin Ecosystem Focus`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **Why does `Config` connect `Src Config` to `Src App`, `Src Ui`, `Src App`, `Src Term`, `Src Llm`, `Src Term`, `Src App`, `Src Config`, `Src App`, `Src App`, `Src App`, `Src App`, `Src App`, `AcpAgentConfig`, `shaper.rs`, `Src Renderer`, `keybind_view.rs`, `.handle_redraw`, `app/ui/mod.rs`, `Mux`, `gpu.rs`, `resolve_color`, `CellVertex`?**
  _High betweenness centrality (0.380) - this node is a cross-community bridge._
- **Why does `FontConfig` connect `Src App` to `Src Config`, `Src Renderer`, `Src Font`, `Src Font`, `.resumed`, `Src App`, `Src App`, `Src App`, `resolve_color`, `Src Llm`, `Src App`, `Src App`?**
  _High betweenness centrality (0.118) - this node is a cross-community bridge._
- **Why does `ColorScheme` connect `Src App` to `Src Config`, `String`, `Src Term`, `Src App`, `Src Ui`, `Src App`, `gpu.rs`, `resolve_color`, `Benches Rasterize.rs`, `Src Config`, `CellVertex`, `Src Ui`?**
  _High betweenness centrality (0.106) - this node is a cross-community bridge._
- **What connects `@modelcontextprotocol/server-filesystem`, `@modelcontextprotocol/server-fetch`, `build_pgo.sh script` to the rest of the system?**
  _115 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Src Config` be split into smaller, more focused modules?**
  _Cohesion score 0.1073170731707317 - nodes in this community are weakly interconnected._
- **Should `Src App` be split into smaller, more focused modules?**
  _Cohesion score 0.12233285917496443 - nodes in this community are weakly interconnected._