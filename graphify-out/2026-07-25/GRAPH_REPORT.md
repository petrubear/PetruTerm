# Graph Report - /Users/edison/Documents/Projects/Rust/PetruTerm  (2026-07-25)

## Corpus Check
- 206 files · ~171,730 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1783 nodes · 3752 edges · 92 communities (84 shown, 8 thin omitted)
- Extraction: 98% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 56 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

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
- Src App
- Src App
- Src Llm
- Scripts Gen Icon.swift
- Src Renderer
- Src Platform
- Src Llm
- Src Term
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

## God Nodes (most connected - your core abstractions)
1. `Config` - 70 edges
2. `UiManager` - 64 edges
3. `FontConfig` - 50 edges
4. `ChatPanel` - 50 edges
5. `App` - 48 edges
6. `Mux` - 48 edges
7. `GpuRenderer` - 45 edges
8. `TextShaper` - 37 edges
9. `ColorScheme` - 32 edges
10. `Terminal` - 27 edges

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
- 1-file cycle: `src/platform/battery.rs -> src/platform/battery.rs`
- 2-file cycle: `src/font/freetype_lcd.rs -> src/renderer/lcd_atlas.rs -> src/font/freetype_lcd.rs`

## Hyperedges (group relationships)
- **Project Operational Context Set** — context_core_active_context_document, context_core_session_state_document, context_quality_technical_debt_document [INFERRED 0.85]
- **Planning and Specification Backbone** — context_specs_build_phases_document, context_specs_build_phases_archive_document, context_specs_term_specs_document [INFERRED 0.75]
- **Release Artifact Chain** — github_workflows_release_document, changelog_document, readme_document [INFERRED 0.65]

## Communities (92 total, 8 thin omitted)

### Community 0 - "Src Config"
Cohesion: 0.06
Nodes (52): EventLoopProxy, Path, String, UiManager, config_dir(), config_path(), ensure_default_configs(), extract_lua_version() (+44 more)

### Community 1 - "Src App"
Cohesion: 0.07
Nodes (26): Runtime, AiPollResult, classify_llm_error(), RenamePrompt, Arc, ChatPanel, EventLoopProxy, Instant (+18 more)

### Community 2 - "Src Term"
Cohesion: 0.06
Nodes (37): Range, command_end_deactivates(), command_start_deactivates(), ctrl_u_clears_before_cursor(), InputShadow, insert(), kill_word(), kill_word_at_start() (+29 more)

### Community 3 - "Src Llm"
Cohesion: 0.07
Nodes (36): char_chunks(), ChatPanel, Path, PathBuf, String, Vec, scan_dir(), scan_files() (+28 more)

### Community 4 - "Src Llm"
Cohesion: 0.08
Nodes (34): BufWriter, ChildStdin, ChildStdout, Error, dispatch_response(), extract_tool_result_text(), extract_tool_result_text_content_blocks(), extract_tool_result_text_fallback_to_json() (+26 more)

### Community 5 - "Src Font"
Cohesion: 0.08
Nodes (35): Attrs, AttrsList, LayoutGlyph, LruCache, Metrics, build_attr_list(), FreeTypeCmapLookup, glyph_to_cache_key() (+27 more)

### Community 6 - "Src Font"
Cohesion: 0.07
Nodes (29): FT_Bitmap, FreeTypeLcdRasterizer, LcdAtlasEntry, LcdPixelMode, Device, Drop, FT_Face, FT_Library (+21 more)

### Community 7 - "Src Ui"
Cohesion: 0.14
Nodes (21): adjust_parent_split(), collect_leaf_infos_impl(), collect_separators_impl(), contains_leaf(), drag_split_ratio(), find_rect(), FocusDir, next_node_id() (+13 more)

### Community 8 - "Src Term"
Cohesion: 0.09
Nodes (23): Block, BlockManager, blocks_in_viewport_filters_correctly(), complete_block_lifecycle(), exit_code_nonzero(), incomplete_block_not_in_viewport(), mgr(), Option (+15 more)

### Community 9 - "Src App"
Cohesion: 0.09
Nodes (23): list_saved_workspaces(), load_workspace(), PaneNodeSnapshot, Box, Option, PathBuf, Result, String (+15 more)

### Community 10 - "Src Ui"
Cohesion: 0.07
Nodes (23): FromStr, Menu, MenuEvent, MenuId, AppMenu, Option, Self, Vec (+15 more)

### Community 11 - "Src App"
Cohesion: 0.08
Nodes (16): App, build_all_pane_instances(), ActiveEventLoop, Instant, Mux, Option, RenderContext, Result (+8 more)

### Community 12 - "Src Term"
Cohesion: 0.12
Nodes (29): Child, EventListener, Fn, OnceLock, pid_t, RawFd, open_pty(), Pty (+21 more)

### Community 13 - "Src Renderer"
Cohesion: 0.09
Nodes (18): PresentMode, GpuRenderer, make_main_atlas_bind_group(), Arc, BindGroup, Buffer, Color, Device (+10 more)

### Community 14 - "Src Llm"
Cohesion: 0.10
Nodes (28): AcpAgentConfig, Vec, AcpSession, build_acp_agent(), PromptMsg, AcpAgent, Instant, JoinHandle (+20 more)

### Community 16 - "Src Ui"
Cohesion: 0.10
Nodes (13): Option, String, SidebarState, label_format_and_truncation(), Default, Into, Option, Self (+5 more)

### Community 17 - "Src Term"
Cohesion: 0.10
Nodes (15): CursorShape, F, R, CursorInfo, process_cwd(), Arc, EventLoopProxy, FairMutex (+7 more)

### Community 18 - "Src App"
Cohesion: 0.12
Nodes (18): ApplicationHandler, SelectionType, App, Arc, Drop, EventLoopProxy, HashMap, Instant (+10 more)

### Community 19 - "Src Llm"
Cohesion: 0.15
Nodes (22): OnceCell, RequestBuilder, CachedJwt, CopilotProvider, CopilotTokenResponse, DeviceCodeResponse, keychain_load(), keychain_save() (+14 more)

### Community 20 - "Src Config"
Cohesion: 0.18
Nodes (25): LuaResult, LuaTable, bytecode_cache_path(), config_stdlib(), drain_lua_toast(), evict_stale_lua_cache(), fire_lua_event(), hash_path() (+17 more)

### Community 21 - "Src App"
Cohesion: 0.13
Nodes (20): brighten(), build_usage_hint(), calculate_row_hash(), colors_approx_eq(), pack_color(), RenderContext, resolve_span_fg(), RowCache (+12 more)

### Community 22 - "Benches Build Instances.rs"
Cohesion: 0.22
Nodes (21): apply_row_offset(), bench_build_frame_hit(), bench_build_frame_hit_large_par(), bench_build_frame_hit_large_serial(), bench_build_frame_miss(), bench_build_row_hit(), bench_build_row_miss(), build_row_vertices() (+13 more)

### Community 23 - "Src App"
Cohesion: 0.13
Nodes (16): Event, InputHandler, ActiveEventLoop, EventLoopProxy, HashMap, Instant, KeyEvent, Modifiers (+8 more)

### Community 24 - "Src App"
Cohesion: 0.22
Nodes (3): App, Option, Result

### Community 25 - "Src Ui"
Cohesion: 0.16
Nodes (13): ContextAction, ContextMenu, ContextMenuItem, default_items(), item(), item_kb(), label_item(), open_default_resets_tab_color_picker_items() (+5 more)

### Community 26 - ".context Specs"
Cohesion: 0.11
Nodes (25): AGENTS Guide, Phase 4 Plugin Ecosystem Focus, Changelog, CLAUDE Instructions, System Map, Active Context, Phase 9 UI Restyle Complete, Session State (+17 more)

### Community 27 - "Src Llm"
Cohesion: 0.13
Nodes (8): App, Option, String, Option, PathBuf, Self, String, ShellContext

### Community 28 - "Src Ui"
Cohesion: 0.13
Nodes (15): StatusBarColors, days_to_ymd(), format_time(), is_leap(), Default, Option, Path, Self (+7 more)

### Community 29 - "Src Llm"
Cohesion: 0.16
Nodes (23): Path, PathBuf, Result, validate_path(), env_vars_parsed(), load_from_paths(), load_global(), load_local() (+15 more)

### Community 30 - "Src Llm"
Cohesion: 0.18
Nodes (16): collect_skill_files(), extract_body(), extract_body_basic(), parse_frontmatter(), parse_frontmatter_basic(), parse_skill_file(), Option, Path (+8 more)

### Community 31 - "Src Renderer"
Cohesion: 0.13
Nodes (7): Queue, Result, ColorAtlas, GlyphAtlas, HashMap, Sampler, TextureView

### Community 32 - "Src Llm"
Cohesion: 0.10
Nodes (6): ActionPayload, AgentAction, parse_action_from_response(), Option, String, PanelState

### Community 33 - "Src Llm"
Cohesion: 0.17
Nodes (17): build_provider(), infer_context_window(), LlmProvider, parse_agent_response(), parse_sse_chunk(), parse_usage(), Arc, Option (+9 more)

### Community 34 - "Src Renderer"
Cohesion: 0.17
Nodes (14): AtlasEntry, AtlasError, dummy_entry(), dummy_key(), evict_cold_keeps_all_when_all_warm(), evict_cold_removes_all_when_all_stale(), evict_cold_removes_old_entries(), pad_r8_rows() (+6 more)

### Community 35 - "Src App"
Cohesion: 0.19
Nodes (7): resolve_line_fg(), RenderContext, Option, ColorScheme, FontConfig, Color, PathBuf

### Community 36 - "Src Llm"
Cohesion: 0.15
Nodes (15): AgentRequest, ApiMessage, build_api_messages(), ChatRequest, OpenAICompatProvider, ApiMessage, Client, Option (+7 more)

### Community 37 - "Src Llm"
Cohesion: 0.22
Nodes (9): AgentStepResult, AgentTool, execute_tool(), Option, Path, String, Value, Vec (+1 more)

### Community 38 - "Src App"
Cohesion: 0.21
Nodes (15): FxHashMap, SelectionRange, cell_in_selection(), FlagHintOverlay, GhostOverlay, push_search_match_truncates_only_after_limit_is_exceeded(), AnsiColor, Option (+7 more)

### Community 39 - "Src Llm"
Cohesion: 0.18
Nodes (16): command_returns_none_on_empty_panel(), command_returns_none_when_only_tool_lines(), command_strips_done_tool_line(), command_strips_in_progress_tool_line(), command_strips_markdown_fence_after_tool_lines(), command_strips_multiple_tool_lines(), command_without_tool_lines_unchanged(), header_action_for_col() (+8 more)

### Community 40 - "Src Llm"
Cohesion: 0.15
Nodes (16): AgentRequest, ApiMessage, build_api_messages(), ChatRequest, keychain_api_key(), OpenRouterProvider, ApiMessage, Client (+8 more)

### Community 42 - "Src Llm"
Cohesion: 0.14
Nodes (5): AiBlock, AiState, Option, Self, String

### Community 43 - "Src App"
Cohesion: 0.14
Nodes (8): ElementState, MouseButton, MouseScrollDelta, PhysicalPosition, ActiveEventLoop, KeyEvent, WindowEvent, WindowId

### Community 44 - "Src Llm"
Cohesion: 0.24
Nodes (9): context_block_empty(), context_block_with_files(), read_md_files(), Option, Path, Self, String, Vec (+1 more)

### Community 45 - "Src App"
Cohesion: 0.30
Nodes (7): PanelMsgParams, RenderContext, ChatPanel, String, dim(), idx_or_default(), T

### Community 46 - "Src App"
Cohesion: 0.16
Nodes (11): Arc, Result, Self, Window, build_font_system(), locate_font_for_lcd(), FontSystem, ID (+3 more)

### Community 47 - "Src App"
Cohesion: 0.31
Nodes (5): Column, Line, SearchMatch, push_search_match(), String

### Community 48 - "Src App"
Cohesion: 0.21
Nodes (8): fetch_git_branch(), list_git_branches_sync(), Duration, Option, Path, String, Vec, UiManager

### Community 49 - "Src Llm"
Cohesion: 0.24
Nodes (11): ConfirmDisplay, Path, Self, Vec, compress_diff(), diff_lines(), DiffKind, DiffLine (+3 more)

### Community 50 - "Benches Rasterize.rs"
Cohesion: 0.36
Nodes (12): bench_rasterize_glyph_ascii(), bench_rasterize_line_ascii(), bench_rasterize_line_ligatures(), bench_rasterize_line_unicode(), make_colors(), make_shaper(), rasterize_one(), CacheKey (+4 more)

### Community 51 - "Benches Search.rs"
Cohesion: 0.41
Nodes (12): bench_search_cold(), bench_search_cold_par(), bench_search_incremental(), build_flat_grid(), build_grid(), filter_matches(), push_search_match(), Criterion (+4 more)

### Community 53 - "Src Config"
Cohesion: 0.23
Nodes (9): RecommendedWatcher, ConfigWatcher, Duration, Option, Path, PathBuf, Receiver, Result (+1 more)

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
Cohesion: 0.18
Nodes (7): RenderContext, AnsiColor, Mux, Option, Result, String, Vec

### Community 58 - "Src Font"
Cohesion: 0.35
Nodes (6): FontLocator, FontPath, Default, Option, PathBuf, Self

### Community 59 - "Src Renderer"
Cohesion: 0.17
Nodes (9): RectUniforms, RoundedRectPipeline, BindGroup, Buffer, Device, Queue, RenderPipeline, Self (+1 more)

### Community 60 - "Benches Shaping.rs"
Cohesion: 0.53
Nodes (9): bench_shape_line_ascii(), bench_shape_line_ascii_cached(), bench_shape_line_ligatures(), bench_shape_line_ligatures_cached(), bench_shape_line_unicode(), make_colors(), make_shaper(), Criterion (+1 more)

### Community 61 - "Src App"
Cohesion: 0.44
Nodes (9): format_csi(), format_fkey(), format_tilde(), Key, Modifiers, Option, Vec, translate_key() (+1 more)

### Community 62 - "Src App"
Cohesion: 0.42
Nodes (4): EventLoopProxy, PathBuf, Result, Self

### Community 63 - "Src Llm"
Cohesion: 0.44
Nodes (5): ChatMessage, ChatRole, Into, Self, String

### Community 64 - "Scripts Gen Icon.swift"
Cohesion: 0.31
Nodes (8): CGColor, CGFloat, CoreGraphics, Foundation, ImageIO, hex(), srgb(), UInt32

### Community 65 - "Src Renderer"
Cohesion: 0.47
Nodes (3): Device, Self, Texture

### Community 66 - "Src Platform"
Cohesion: 0.46
Nodes (7): c_void, BatteryStatus, cfdict_i32(), cfdict_str(), query(), Option, String

### Community 67 - "Src Llm"
Cohesion: 0.39
Nodes (7): is_trusted(), Option, Path, PathBuf, Result, trust(), trust_file()

### Community 68 - "Src Term"
Cohesion: 0.43
Nodes (6): NamedColor, dim(), resolve_color(), resolve_indexed(), resolve_named(), AnsiColor

### Community 69 - "Config Default"
Cohesion: 0.40
Nodes (5): fetch, filesystem, npx, @modelcontextprotocol/server-fetch, @modelcontextprotocol/server-filesystem

### Community 72 - "Src I18n.rs"
Cohesion: 0.67
Nodes (3): detect_locale(), init(), String

### Community 73 - "Src Main.rs"
Cohesion: 0.83
Nodes (3): inherit_login_shell_env(), main(), Result

### Community 74 - "Assets Appicon.png"
Cohesion: 1.00
Nodes (3): PetruTerm App Icon, Cursor Block, Terminal Prompt Chevron

## Ambiguous Edges - Review These
- `Phase 9 UI Restyle Complete` → `Phase 4 Plugin Ecosystem Focus`  [AMBIGUOUS]
  .context/core/ACTIVE_CONTEXT.md · relation: conceptually_related_to

## Knowledge Gaps
- **20 isolated node(s):** `@modelcontextprotocol/server-filesystem`, `@modelcontextprotocol/server-fetch`, `build_pgo.sh script`, `bundle.sh script`, `ci-local.sh script` (+15 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **8 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **What is the exact relationship between `Phase 9 UI Restyle Complete` and `Phase 4 Plugin Ecosystem Focus`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **Why does `Config` connect `Src Config` to `Src App`, `Src Llm`, `Src App`, `Src Ui`, `Src App`, `Src Term`, `Src Renderer`, `Src Llm`, `Src Term`, `Src App`, `Src Config`, `Src App`, `Src App`, `Src App`, `Src App`, `Src App`, `Src App`, `Src App`, `Src App`?**
  _High betweenness centrality (0.456) - this node is a cross-community bridge._
- **Why does `FontConfig` connect `Src App` to `Src Config`, `Src Font`, `Src Font`, `Src App`, `Src App`, `Src Llm`, `Src App`, `Benches Rasterize.rs`, `Src App`, `Benches Build Instances.rs`, `Src App`, `Benches Shaping.rs`?**
  _High betweenness centrality (0.126) - this node is a cross-community bridge._
- **Why does `App` connect `Src App` to `Src Config`, `Src Platform`, `Src Llm`, `Src Ui`, `Src Ui`, `Src App`, `Src Ui`, `Src Config`, `Src App`, `Src App`, `Src Llm`, `Src Ui`?**
  _High betweenness centrality (0.089) - this node is a cross-community bridge._
- **What connects `@modelcontextprotocol/server-filesystem`, `@modelcontextprotocol/server-fetch`, `build_pgo.sh script` to the rest of the system?**
  _20 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Src Config` be split into smaller, more focused modules?**
  _Cohesion score 0.06277665995975855 - nodes in this community are weakly interconnected._
- **Should `Src App` be split into smaller, more focused modules?**
  _Cohesion score 0.07023705004389816 - nodes in this community are weakly interconnected._