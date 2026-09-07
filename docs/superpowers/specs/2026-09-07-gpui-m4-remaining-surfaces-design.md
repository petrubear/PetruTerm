# M4 — Remaining Surfaces: Design

**Parent spec:** `docs/superpowers/specs/2026-08-30-gpui-chrome-migration-design.md` (M4: "Command
palette, context menu, search bar, info overlay/toasts.")

**Status:** approved by the user in conversation (design summary + a-d decomposition), not yet
reviewed as a written document.

---

## 1. Scope

Info overlay is already done — built as part of M3d (`src/gpui_shell/info_overlay.rs`), reused by
this milestone's own popups where relevant. M4's real remaining scope is four independently
dogfoodable sub-milestones, decomposed the same way M3 (M3a–d) was, since that decomposition worked
well this session:

- **M4a — Command palette.** `Leader o`, fuzzy-matched action list.
- **M4b — Search bar.** `Cmd+F`, terminal-grid text search with match highlighting.
- **M4c — Context menu (scoped down).** Right-click menu: Copy/Paste/Clear/SetTabColor only.
- **M4d — Toasts.** Transient top-right messages, Lua-triggered.

Each ships as its own plan under this shared spec, mirroring M3c/M3d's relationship to the M3 spec.

## 2. What's reusable vs. new

`CommandPalette` (`src/ui/palette/mod.rs`) and `SearchBar` (`src/ui/search_bar.rs`) are pure,
engine-agnostic state machines — no wgpu/rendering coupling. They are **used directly, never
copied**, the same relationship M3b/M3d established for `ChatPanel`/`SkillManager`/`McpManager`.
Only their render + keybind wiring is new work.

`ContextMenu`/`ContextAction` (`src/ui/context_menu.rs`) is also engine-agnostic, but several of its
actions depend on wgpu-only infrastructure `gpui_shell` doesn't have (see §5). `Mux::
search_active_terminal` (`src/app/mux/mod.rs`) has the real, rayon-parallelized grid-search
algorithm, but it's a method on `Mux`, which `gpui_shell` doesn't use — see §4.2.

## 3. M4a — Command Palette

**Files (new):** `src/gpui_shell/palette.rs` (render + `GpuiShellRoot` integration).

**State:** `crate::ui::palette::CommandPalette`, stored as a new `GpuiShellRoot` field, constructed
in `new()` the same way `sidebar`/`chat`/`ai_block` are.

**Render:** a centered modal popup, structurally similar to `InfoOverlay` (dimmed backdrop,
`cx.stop_propagation()` on backdrop clicks) but with a `TextInput` (M3a) query field at the top and
a live-filtered, scrollable result list below (reuse the `.id(...)` + `overflow_y_scroll()` +
`impl IntoElement` pattern M3d's sidebar sections established). Each row shows the action's name and
keybind (if any), matching `CommandPalette::results`' `PaletteAction` shape.

**Keybind:** `Leader o` opens it (new `LeaderAction::OpenCommandPalette` variant, wired in
`leader.rs`/`leader_dispatch.rs`, same shape as `ToggleWorkspaceSidebar`). While open, it is a
genuine text-input-focused modal: typing filters (`CommandPalette::type_char`/`backspace`),
Up/Down move selection (`select_up`/`select_down`), Enter confirms (`confirm()` returns
`Option<Action>`), Escape closes. Focus-guard-wise this is the SAME shape as the workspace-rename
`TextInput` (M3a/M3c): keyed on the query `TextInput`'s own `is_focused(window)`, not a
visibility flag — typing must never leak into the terminal underneath.

**Action dispatch bridge:** `confirm()` returns a wgpu-native `Action` enum value (`src/ui/palette/
actions.rs`, ~30 variants). `gpui_shell` needs a `fn dispatch_palette_action(&mut self, action:
Action, window, cx)` that maps each variant it already supports onto the existing `gpui_shell`
primitive:

| `Action` variant | Maps to |
|---|---|
| `NewTab`/`CloseTab`/`NextTab`/`PrevTab`/`RenameTab` | `LeaderAction` equivalents, called via `dispatch_leader_action` |
| `NewWorkspace`/`CloseWorkspace`/`RenameWorkspace`/`NextWorkspace`/`PrevWorkspace` | same |
| `SplitHorizontal`/`SplitVertical`/`ClosePane`/`ZoomPane`/`FocusPane(dir)` | same |
| `ToggleAiPanel`/`FocusAiPanel` | same |
| `ToggleFullscreen` | window-level, needs a `gpui::Window` fullscreen call — new, small |
| `Quit` | `cx.quit()` — new, trivial |
| `ToggleStatusBar` | `render.rs` already reads `self.config.status_bar.enabled` fresh every frame (no separate runtime flag) — the action just flips that bool directly, no new state needed |
| `OpenConfigFile`/`OpenConfigFolder`/`ReloadConfig` | new, small (open a path via the OS, or reuse the hot-reload watcher's existing reload path) |
| `SwitchToTab(usize)` | `tabs.switch_to_index(n)`, already exists |

Every other variant (`ExplainLastOutput`/`FixLastError`/`UndoLastWrite`/`ClearAiContext` — M3b
explicitly deferred these; `TrustLocalMcp`; `GitCheckout`/`ExpandSnippet` — snippets and git-branch
picker aren't ported to `gpui_shell` yet; `SaveWorkspace`/`OpenSavedWorkspaces`/`RestoreWorkspace` —
workspace persistence isn't ported yet; `ToggleAiMode` — legacy alias) is **filtered out of the
palette's action list entirely** for `gpui_shell` (not shown, not dispatchable), rather than shown
and silently no-op'd. See §5 for the deferred list.

## 4. M4b — Search Bar

**Files (new):** `src/gpui_shell/search_bar.rs` (render); modifies `terminal_element.rs` (paint
match highlights).

**State:** `crate::ui::search_bar::SearchBar`, new `GpuiShellRoot` field.

### 4.1 UI

A small top-of-pane-area overlay (not a full modal — the terminal stays visible and scrollable
underneath, closer to `chat_panel`'s non-modal drawer than `InfoOverlay`'s blocking one), with a
`TextInput` query field, a match-count label (`SearchBar::count_label()`, already implemented), and
Enter/Shift+Enter (or a small prev/next affordance) stepping through `next_match`/`prev_match`.
`Cmd+F` opens/closes it (matches AGENTS.md's existing keybind table entry — this is filling in a gap,
not changing documented behavior).

### 4.2 Search execution — the real decision

`Mux::search_active_terminal(&self, query: &str) -> (Vec<SearchMatch>, bool)` is a real,
rayon-parallelized implementation (serial grid-read under the terminal lock, then a parallel scan)
with its own regression test (`push_search_match_truncates_only_after_limit_is_exceeded`). It is a
method on `Mux`, which `gpui_shell` does not use (see M3c's own design note on why `WorkspaceManager`
mirrors `Mux` rather than wrapping it).

**Decision: extract the algorithm into a free function.** Refactor `Mux::search_active_terminal`
into a free function `fn search_terminal(terminal: &Terminal, query: &str) -> (Vec<SearchMatch>,
bool)` in `src/term/` (or wherever `Terminal` itself lives), with `Mux::search_active_terminal`
becoming a one-line wrapper (`search_terminal(self.active_terminal()?, query)`) so the wgpu build's
own behavior and its existing test are unaffected. `gpui_shell` then calls the free function directly
on whichever `Terminal` the active pane holds — the exact "used directly, never copied" relationship
this spec keeps reaching for, applied to a case that needed one small refactor to make it true. This
is the recommended approach over duplicating the algorithm (drift risk between two copies of a
parallelized scan) or leaving it Mux-only (blocks reuse entirely).

### 4.3 Rendering matches

`TerminalGridElement`'s paint path (M1/M2 work, `terminal_element.rs`) needs to draw a highlight
rect under each `SearchMatch`'s cell range, plus a distinct highlight for the "current" match
(`SearchBar::current_match()`). This is the one piece of real new rendering work in M4b — everything
else is state-machine wiring + a `TextInput`-based overlay, both established patterns by now.

## 5. M4c — Context Menu (Scoped Down)

**Files (new):** `src/gpui_shell/context_menu.rs` (render + hit-test); modifies `mouse.rs`
(right-click handling, currently absent entirely).

**State:** `crate::ui::context_menu::ContextMenu`, new `GpuiShellRoot` field.

**In scope this milestone** — the four `ContextAction` variants with existing `gpui_shell`
primitives to call:

| `ContextAction` | `gpui_shell` primitive |
|---|---|
| `Copy` | `cx.write_to_clipboard(...)`, already used in `mouse.rs` for selection copy |
| `Paste` | `cx.read_from_clipboard()`, already used in `input.rs`'s `Cmd+V` handler |
| `Clear` | `Terminal::clear_screen_and_scrollback()` — **exists but `Cmd+K` itself isn't wired in `input.rs` yet** (a real gap versus AGENTS.md's own keybind table); this milestone adds both the keybind and the context-menu item, sharing one code path |
| `SetTabColor(idx, color)` | `TabManager::set_tab_color(idx, color)`, already exists (built for the tab-color picker, not yet exposed via right-click) |

**Explicitly out of scope, deferred** (see §6): `SendToChat`, `CopyLastCommand`, `CopyBlockOutput`,
`ReRunCommand`, `OpenLink`, `CopyLink`. All six need infrastructure `gpui_shell` has zero of yet:
command-block tracking (`src/term/blocks.rs`'s `Block` concept — never referenced anywhere in
`gpui_shell`) and hover-link detection (no URL/path hover state exists in `mouse.rs`). Building
either is a real, separate design effort — not something to improvise as a side effect of M4c's
right-click menu. `ContextMenu::open_default`/`open_with_link`/`open_with_block`/`open_exit_info`
constructors that build menus containing these items are simply not called; only a new, smaller
constructor (or `open_default`'s own item list trimmed) builds the Copy/Paste/Clear/SetTabColor-only
menu this milestone ships.

**Render:** a simple positioned popup (open at the click's row/col → pixel position, same conversion
`pane_view.rs` already does for cursor/selection), listing the in-scope items; `ContextMenu::
hit_test(col, row)` (already implemented) resolves a click back to an action.

## 6. M4d — Toasts

**Files (new):** `src/gpui_shell/toast.rs` (state + render).

**State:** a new, small `GpuiShellRoot` field, `toast: Option<(String, Instant)>` — same shape the
wgpu build's own `app_state.rs` already uses (`self.toast = Some((msg, deadline))`, drained from
`crate::config::lua::drain_lua_toast(lua)` after each Lua event fires). `gpui_shell`'s own poll loop
(`poll.rs`, already responsible for cursor blink and leader-deadline expiry) is the natural place to
drain the Lua toast queue and clear an expired toast on tick, mirroring how the wgpu build's own
`frame.rs` clears `self.toast` once its deadline passes.

**Render:** a small, non-modal, non-interactive top-right label — no backdrop, no focus, no
`stop_propagation` (a toast is a passive notification the user can click straight through, unlike
`InfoOverlay`). Auto-dismisses on the next poll tick after its deadline; no explicit close
affordance needed since nothing this milestone drives *into* a toast staying open (no hover-to-dismiss,
no click target).

## 7. Deferred (explicitly out of scope for M4a–d)

Recorded here so a later milestone (or a future audit) has one place to check, rather than
rediscovering these by reading dispatch-bridge code:

- **Command-block tracking** (`src/term/blocks.rs`'s `Block`) — not referenced anywhere in
  `gpui_shell`. Blocks `CopyBlockOutput`/`ReRunCommand`/`SendToChat` (context menu) and any future
  "jump to last command's output" feature.
- **Hover-link detection** — no URL/path hover state in `mouse.rs`. Blocks `OpenLink`/`CopyLink`
  (context menu).
- **Snippets** (`Action::ExpandSnippet`) — not ported to `gpui_shell`. Blocks the palette's snippet
  actions.
- **Saved workspaces / workspace persistence** (`SaveWorkspace`/`OpenSavedWorkspaces`/
  `RestoreWorkspace`) — confirmed absent from `gpui_shell` (no persistence/snapshot code anywhere in
  `src/gpui_shell/`), despite being a designed, working wgpu-build feature per project memory. These
  three stay filtered from the palette until a future milestone ports it.
- **Git-branch picker** (`Action::GitCheckout`) — `open_with_items` (custom-list palette mode) exists
  in the reused `CommandPalette` state machine, but nothing in `gpui_shell` populates it with branch
  names yet.
- **AI actions already deferred by M3b**: `ExplainLastOutput`, `FixLastError`, `UndoLastWrite`,
  `ClearAiContext`, `TrustLocalMcp` — carried forward from that milestone's own scope note, not
  reopened here.
- **`ToggleAiMode`** — the palette's own doc comment calls it a legacy alias for `ToggleAiPanel`;
  filtered out to avoid offering two palette entries for one action.

## 8. Manual testing required (cannot be verified from the agent sandbox)

Per this project's established testing discipline (no painting/hit-testing tests — GPU windows can't
be driven interactively from here; confirmed this session: screenshots of a launched instance work,
but synthetic keyboard/mouse input does not, blocked both by missing Accessibility permission and by
the harness's own permission classifier). Every item below needs the user's own hands:

**M4a — Command palette:**
- `Leader o` opens the palette and focuses the query field immediately (type without clicking first).
- Typing filters the list live; fuzzy matching feels right (not just prefix matching).
- Up/Down move the highlighted row, wrapping at both ends.
- Enter on a highlighted row runs that action and closes the palette.
- Escape closes without running anything.
- Every action in the dispatch table (§3) actually does the right thing — not just "doesn't crash."
- Clicking into the terminal while the palette is open does NOT get intercepted (the standing
  focus-guard regression class).

**M4b — Search bar:**
- `Cmd+F` opens/closes the search bar.
- A query with real matches highlights them in the terminal grid, current match visually distinct
  from the rest.
- Match count label is accurate.
- Next/prev navigation actually scrolls to and highlights the right match.
- Search over scrollback (not just the visible viewport) works.
- Performance on a long scrollback doesn't visibly stall input (the rayon-parallel path's whole
  reason for existing).

**M4c — Context menu:**
- Right-click opens the menu at the click position, not offset/clipped at screen edges.
- Copy/Paste/Clear/SetTabColor each do the right thing.
- `Cmd+K` (now wired) clears screen + scrollback, matching AGENTS.md's documented behavior.
- Clicking elsewhere closes the menu without triggering an action.
- Tab-color picker submenu (if built this milestone rather than deferred further) actually changes
  the tab's underline color, matching M3d's own accent-underline fix.

**M4d — Toasts:**
- A Lua-triggered toast actually appears top-right and auto-dismisses after its configured duration.
- Multiple toasts firing in quick succession don't overlap illegibly or crash the poll loop.
- A toast doesn't block or intercept clicks/keys meant for the terminal underneath (it's
  deliberately non-modal — this is the one new surface this milestone adds that is NOT keyed on any
  focus guard at all, since it never takes focus).

## 9. Global constraints (carried forward from M3d, still binding)

- 400-line module limit — watch cumulative growth across tasks within each sub-milestone, the same
  class of miss M3c's `actions.rs` and M3d's `input.rs`/`render.rs` each hit once.
- `scripts/ci-local.sh` must stay green after every task; `#[allow(dead_code)]` (narrowly scoped,
  comment naming the removing task) for fields/methods built ahead of their first caller.
- Commit format: `type: Message.` per `AGENTS.md`.
- Key/focus guards key on real focus (`is_focused(window)`), never on visibility/open state. Unlike
  `InfoOverlay` (M3d), M4a's command palette does NOT get the visibility exception: it has a real
  `TextInput` query field that grabs real focus, so its keyboard guard follows the tab/workspace-
  rename precedent (§3) — keyed on that `TextInput`'s own `is_focused(window)`, exactly like every
  other text-input-holding surface in this codebase. `InfoOverlay`'s exception only ever applied to
  itself, precisely because it grabs no `FocusHandle` at all (nothing to type into) — a condition
  M4a's palette doesn't share. The palette backdrop's `cx.stop_propagation()` (blocking clicks from
  reaching what's behind it) is a separate, unconditional mouse-click behavior, not a keyboard guard,
  and needs no focus check of its own. M4b's search bar and M4d's toast are both non-modal (terminal
  stays interactive underneath) and need no backdrop-blocking behavior at all.
- Tests for logic only — no painting/layout/hit-testing tests; every item in §8 is a dogfood step,
  not a unit test to write.
