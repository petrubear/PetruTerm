# M3 — Sidebars & AI Panel: Design

**Parent spec:** `docs/superpowers/specs/2026-08-30-gpui-chrome-migration-design.md` (M3: "Workspace nav
+ AI chat panel, with real gpui styling/animation — the feature that motivated this migration.")

**Survey this design argues from:** `.superpowers/sdd/2026-09-04-gpui-m2-core-chrome/m3-survey.md`
(read-only map of the wgpu implementation; git-ignored, so its load-bearing findings are restated here).

**Status:** proposed, awaiting approval.

---

## 1. What the survey changed about the plan

Three findings reshape this milestone before any code is written.

**Roughly half the surface ports verbatim.** `ChatPanel` (919 lines + 277 picker, with 12 unit tests),
`SidebarState`, `AiBlock`, all of `src/llm/` (providers, streaming, ACP, skills, MCP, steering,
markdown parsing), workspace CRUD, and snapshot persistence contain zero winit/wgpu references. They
are copied, not rewritten — the precedent `status_bar/mod.rs`'s own header already documents for
`StatusBar::build`. What must be rewritten is drawing and hit-testing: `src/app/renderer/chat.rs`
(1567 lines) and the sidebar's draw/hit-test paths, all of it `RoundedRectInstance` pixel math over
terminal cell coordinates.

**"Per-pane chat" does not exist.** `set_active_terminal` is `pub fn set_active_terminal(&mut self,
_id: usize) {}` — an empty no-op called every frame; `active_panel_id()` returns a hardcoded `0`; every
producer sends `panel_id = 0`. Comments, and this project's own memory, describe per-pane chat as a
working feature. It is aspirational plumbing. **M3 ports one global conversation and deletes the dead
`panel_id` routing rather than reproducing it.** The project memory asserting otherwise should be
corrected when this lands.

**`gpui_shell` has no workspace concept at all.** `GpuiShellRoot` holds `tabs` + `tab_panes` — exactly
one workspace's worth — with no `Vec<Workspace>`, no `Mux` equivalent, no persistence wiring. "Port the
workspace sidebar" therefore means "introduce a workspace layer into the shell root," which touches the
tab/pane structure M2 just stabilized. This is the milestone's largest architectural risk and drives
the slicing below.

---

## 2. Slicing

M3 splits into four sub-milestones, each independently dogfoodable, ordered to deliver the motivating
feature first and defer the invasive structural change until the value is already banked.

| | Scope | Why here |
|---|---|---|
| **M3a** | Reusable text-input primitive | Cross-cutting prerequisite. Nothing input-heavy can land without it, and it is already blocking M2's deferred `RenameTab`. |
| **M3b** | AI chat panel — drawer, rendering, streaming, slash commands, markdown; inline `Ctrl+Space` block folded in | The feature that motivated the migration. Depends only on M3a, not on workspaces. |
| **M3c** | Workspace layer in `GpuiShellRoot` + workspace sidebar drawer (Workspaces section only) | The invasive structural change, taken once the panel is already working and dogfooded. |
| **M3d** | Sidebar's MCP / Skills / Steering sections + `InfoOverlay` | Genuinely separable surface; the sidebar is 4 sections, not 1, and this is the honest remainder. |

The parent spec's M3 line ("workspace nav + AI chat panel") is fully covered by M3a–M3c; M3d completes
the sidebar the spec's one-liner understates.

---

## 3. Decisions

### 3.1 Text input: port gpui's own reference implementation, as an `Entity`

gpui 0.2.2 ships `examples/input.rs` (746 lines): a `TextInput` implementing `EntityInputHandler` with
selection ranges, **IME marked-range support**, cut/copy/paste, word boundaries, and a custom element
that paints cursor and selection. We port that rather than hand-rolling, and keep its IME support —
the wgpu `ChatPanel` edits char-by-char through `type_char`, so dead keys and CJK input are areas where
the gpui build can be *better* than the original, not merely at parity.

This is the one place M3 deliberately departs from the established `gpui_shell` pattern of "plain state
struct + free `render_*` function". Text input needs focus handling and an `EntityInputHandler`
registration, which require a real gpui `Entity`. Every other surface in this milestone keeps the
existing pattern.

Consumers: chat input (M3b), workspace rename (M3c), file-picker query (M3b), and — as a free
side-effect — M2's deferred `RenameTab`, which M3a should close out.

### 3.2 Markdown: native gpui layout for messages, fixed-width for the input

`parse_markdown(content, width, state)` wraps at parse time to a character count, baking terminal-grid
assumptions into the output. Split the decision by surface:

- **Message list → native gpui wrapping.** Call `parse_markdown` with an effectively unbounded width,
  use its `AnnotatedLine`/`SpanKind` model purely for *styling* (bold/italic/code/syntax), and let gpui
  lay out and wrap. This is what "real gpui styling" in the parent spec means; a chat panel is not a
  terminal grid and should not be pinned to one.
- **Input row → keep fixed-width monospace.** `ChatPanel`'s `cursor_visual_pos`/`visual_pos_to_cursor`/
  `cursor_up`/`cursor_down` assume fixed-width wrapping. Keeping the input monospace lets that cursor
  math port unchanged, and a monospace composer in a terminal is correct anyway.

### 3.3 Layout: flex siblings, never a manual viewport rect

Do **not** port `resize_terminals_for_panel` (`src/app/frame.rs:172`, ~30 call sites). It exists because
the wgpu app has no layout engine. gpui already solves it: `pane_view.rs`'s `fit_terminal`, driven by
`on_children_prepainted`, resizes each PTY to whatever box taffy actually gave it, every layout pass.
Sidebar and panel become flex siblings of the pane tree in `render.rs`, and terminal reflow on toggle
is free. This deletes roughly 30 call sites' worth of manual bookkeeping rather than porting it.

### 3.4 Drawers, animated

Both surfaces are collapsible drawers, VSCode-style — a standing user requirement, recorded in project
memory: closed means the terminal takes the full width; open means the drawer takes its width and the
terminal reflows. The wgpu build has no animation anywhere (toggling is an instant 0→28-column jump).
The parent spec asks M3 for "real gpui styling/animation", so the drawers animate here. Animation is
the one thing in this milestone with no wgpu reference to port — it is new work, deliberately.

### 3.5 AI streaming: a channel owned by the shell root, drained in the poll loop

No static-slot bridge. The `config_watch.rs` `Mutex<Option<T>>` + `AtomicBool` pattern exists only
because the config watcher is spawned in `main()` before any entity exists and has nothing to hold a
channel on. The AI task is spawned *from a method on the shell root*, so it can be handed a channel end
the root already owns — the same shape as `terminals` + the poll loop's existing `try_recv` drain.

Concretely: keep the existing `crossbeam_channel` pair (it is `Send`), store it on the shell root,
spawn onto the `tokio_rt` field `GpuiShellRoot` already has, and add draining as a sixth responsibility
of the 33ms poll loop beside config reload, PTY events, cursor blink, leader deadline, and status-bar
refresh. Drop the `(panel_id, event)` tuple per §1. Drop the winit `EventLoopProxy` wake entirely —
the poll tick is the wake.

### 3.6 Workspace layer mirrors `Mux`

`GpuiShellRoot` gains a `Workspace { tabs: TabManager, tab_panes: Vec<PaneForest> }` grouping and holds
`Vec<Workspace>` + an active index, mirroring the wgpu `Mux` whose CRUD logic (`cmd_new_workspace`,
`cmd_switch_workspace`, …) is fully engine-agnostic and ports onto it directly. `terminals` /
`wakeup_gates` stay flat, keyed by terminal id, since ids are already globally unique and the poll loop
wants one map to walk.

**This is the riskiest change in M3**, because every index path M2 hardened (`tab_panes[active]`,
`close_tab_at`, `reap_pane`, `on_terminal_exited`) gains a workspace dimension. M2 already paid for one
bug of exactly this shape — `TabManager::close_tab` not shifting `active` — so M3c carries an explicit
requirement: regression tests for cross-workspace index invalidation, in the same style as the existing
`tab_manager_tests`.

---

## 4. File structure

Following `status_bar/` and `panes/` precedent, and the 400-line convention:

```
src/gpui_shell/
  text_input/         M3a  entity, EntityInputHandler impl, element (cursor/selection paint)
  chat_panel/         M3b  state port, render (header/messages/input/zero-state/pills), streaming, slash
  ai_block/           M3b  inline Ctrl+Space surface (small; separate state machine from ChatPanel)
  workspace.rs        M3c  Workspace grouping + Mux-equivalent CRUD
  sidebar/            M3c/d  drawer mechanics, Workspaces section, then MCP/Skills/Steering + InfoOverlay
```

`chat.rs`'s 1567 wgpu lines become several files here by necessity — budget that as real work, not
incidental refactoring.

## 5. Keybinds this milestone adds

`leader.rs`'s `LeaderAction` is deliberately narrow (10 variants) and its own doc comment names this
milestone as the one that widens it. M3 adds: `Leader a` sub-prefix (`a`/`e`/`f`/`z`, plus `c` which
exists in code but is missing from `AGENTS.md`), `Ctrl+Space`, `Leader s` and the `Leader e e` chord,
`Leader w`, and the `Leader W` sub-prefix (`&`/`,`/`j`/`k`/`s`/`L` — `L` also undocumented). Reconcile
`AGENTS.md`'s table against the code's actual bindings as part of M3d.

## 6. Testing

Unchanged from the parent spec's approach: unit tests for logic only — the ported `ChatPanel` tests keep
running as-is, cross-workspace index invalidation gets new tests in M3c. No tests for painting, layout,
or hit-testing; those are dogfooded per sub-milestone, since GPU windows cannot be captured from the
agent sandbox. Every sub-milestone ends with a dogfood checkpoint and `scripts/ci-local.sh` green.

## 7. Exit criteria

The AI panel opens as an animated drawer, holds a real streaming conversation with markdown rendering
and slash commands, and accepts IME text; `Ctrl+Space` gives the inline block. The workspace sidebar
opens as an animated drawer, lists workspaces, and supports create/switch/rename/close with the terminal
reflowing automatically; MCP/Skills/Steering sections and `InfoOverlay` work. Every keybind above is
manually exercised. `gpui-petruterm` is usable as a daily driver for everything except the M4 surfaces
(command palette, context menu, search bar, toasts).

## 8. Risks

1. **Workspace restructuring (M3c)** — touches M2's hardened index paths. Mitigated by ordering it after
   the panel ships and by requiring regression tests.
2. **Markdown re-layout (M3b)** — decoupling `parse_markdown` from character-count wrapping may surface
   assumptions beyond the wrap itself. Mitigated by keeping the input row fixed-width.
3. **Scope of `chat.rs`** — 1567 lines is the largest single rewrite in the migration so far.
4. **IME** — the wgpu build's char-by-char input may hide behavioral differences; worth explicit dogfood
   with a non-ASCII input method.
