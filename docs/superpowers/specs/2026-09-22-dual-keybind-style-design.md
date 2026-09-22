# Dual Keybind Style Design

## Goal

Let users choose between two complete keybinding schemes — the current tmux-style,
leader-key-driven bindings, and a new "normal" style using direct macOS Cmd-combos familiar
from Terminal.app/iTerm2 — via a single config property, defaulting to the current tmux
behavior. Full action parity: every action reachable today (including the ones hardcoded as
sub-leader sequences, not expressed in `config.keys`) gets a direct-key equivalent under
"normal" style.

## Scope

Included:

- New `config.keybind_style` property (`"tmux"` | `"normal"`, default `"tmux"`), set in
  `keybinds.lua`.
- A second, complete `config.keys` table for "normal" style, covering every action currently
  reachable via the leader system — both the ones already in `config.keys` (tabs, splits, pane
  focus) and the ones hardcoded as Rust sub-leader sequences (AI panel controls, workspace
  management, sidebar toggle, pane zoom, pane resize).
- A new direct-bind resolution path in both binaries (wgpu `petruterm` and `gpui-petruterm`),
  independent of the leader FSM, keyed on `(mods, key)` rather than a single leader-prefixed
  character.
- A mods-string parser (`"CMD|SHIFT"` → a structured modifier set) — doesn't exist today,
  since the only mods value ever matched is the literal string `"LEADER"`.
- Leader-key passthrough: when `keybind_style == "normal"`, the configured leader key
  (`Ctrl+F` by default) no longer intercepts input at all — it reaches the shell like any
  other keystroke, instead of entering a dead wait-for-sequence state.

Excluded:

- No changes to the existing leader FSM or its hardcoded sub-leader dispatch
  (`src/app/input/mod.rs`, `src/gpui_shell/leader_dispatch.rs`) — "tmux" style keeps working
  exactly as it does today, byte-for-byte.
- No UI for editing keybinds interactively (command-palette-driven rebinding, etc.) — this is
  config-file-only, same as today's `config.keys`.
- No attempt to auto-migrate a user's already-customized `keybinds.lua`; the existing
  managed-file version-bump mechanism (`update_managed_configs`) already overwrites
  `keybinds.lua` on a bundled-version bump, unchanged by this work.

## Current Problem

PetruTerm's keybindings are tmux-style by design (leader key + mnemonic sub-sequences) — a
deliberate choice documented in `[[project_gpui_chrome_migration]]`'s own migration history
("it's why they wrote their own terminal"). This is unfamiliar and unwelcoming to users coming
from Terminal.app/iTerm2 who have never used tmux and don't want to learn a leader-key
vocabulary just to use a terminal emulator. There is currently no way to use PetruTerm with
conventional, direct macOS shortcuts at all: `config.keys`'s `mods` field accepts any string,
but only entries where `mods == "LEADER"` (case-insensitive) are ever read by either binary's
input dispatch (`config::keybind_view::leader_bindings_view`) — a `mods = "CMD"` entry is
silently inert today.

Beyond that, a meaningful chunk of the leader vocabulary isn't data-driven at all: the AI panel
sub-leader (`leader a a/c/e/f/z`), the workspace sub-leader (`leader W n/&/,/j/k/s/L`), and a
few single-key leader actions (`leader s`/`z`/`w`, `leader` + Option+Arrows for pane resize) are
hardcoded two-key-sequence FSM logic in `src/app/input/mod.rs` (wgpu) and mirrored in
`src/gpui_shell/leader_dispatch.rs`/`standalone_keys.rs` (gpui) — they don't go through
`config.keys` even for tmux style. Full parity requires giving all of these a direct-bind
equivalent, not just the ones already expressed as leader entries in `keybinds.lua`.

## Proposed Design

### 1. `keybind_style` config property and schema

New enum in `src/config/schema.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeybindStyle {
    #[default]
    Tmux,
    Normal,
}
```

`Config` gains `pub keybind_style: KeybindStyle`. Parsed in `src/config/lua.rs` the same way
`TitleBarStyle` already is (string → enum, defaulting to `Tmux` on anything unrecognized or
absent) — same pattern, same file, same error handling.

### 2. One `keybinds.lua`, both tables, switch at the top

`config.lua`'s module chain calls each file's `apply_to_config(config)` in a fixed order
(`ui`, `perf`, `keybinds`, `llm`, `snippets`, `notifications`) — modules aren't swapped by
filename, they're plain Lua functions mutating a shared table. So this isn't "ship a second
file and pick one by name"; it's one `keybinds.lua`, holding both `config.keys` tables,
choosing between them with a plain `if` on its own property:

```lua
function module.apply_to_config(config)
  config.keybind_style = "tmux"  -- "tmux" | "normal"
  config.leader = { key = "f", mods = "CTRL", timeout_ms = 1000 }
  config.keyboard = { option_as_meta = false }

  if config.keybind_style == "tmux" then
    config.keys = { --[[ current leader-based table, unchanged ]] }
  else
    config.keys = { --[[ new direct Cmd-combo table, below ]] }
  end
end
```

No cross-file load-order dependency, no second file to keep in sync, no risk of the property
being read before it's set.

### 3. Direct-bind resolution (`config::keybind_view`)

New view function alongside the existing `leader_bindings_view`:

```rust
pub struct DirectBindingsView {
    pub bindings: Vec<KeyBind>, // mods != "LEADER", case-insensitive
}

pub fn direct_bindings_view(config: &Config) -> DirectBindingsView { ... }
```

Plus a new mods-string parser, since today the only mods comparison anywhere in the codebase is
a literal `eq_ignore_ascii_case("LEADER")` — nothing parses `"CMD|SHIFT"` into a structured
modifier set yet:

```rust
bitflags! {
    pub struct Mods: u8 {
        const CMD    = 0b0001;
        const SHIFT  = 0b0010;
        const CTRL   = 0b0100;
        const OPTION = 0b1000;
    }
}
pub fn parse_mods(s: &str) -> Mods; // "CMD|SHIFT" -> Mods::CMD | Mods::SHIFT
```

Both binaries build a `(Mods, key) -> Action` lookup table once at config-load/hot-reload time
(mirroring `leader.rs`'s existing `leader_map` builder), and check it on every key event — this
check is independent of the leader FSM, not gated behind any prefix state.

### 4. Dispatch wiring, per binary

- **wgpu (`src/app/input/mod.rs`)**: new direct-bind lookup runs before the leader-prefix
  branch (but after the existing hardcoded system shortcuts — Cmd+C/V/Q/K/F/1-9 — which stay
  exactly as they are, unconditionally, regardless of `keybind_style`). Matched action
  dispatches through the same `ui.handle_palette_action` call the sub-leader handlers already
  use.
- **gpui (`src/gpui_shell/`)**: new `direct_bind.rs`, same shape as `leader.rs`'s existing
  table-builder, consulted from the same key-handling entry point `standalone_keys.rs` already
  uses for non-leader hardcoded shortcuts (e.g. Ctrl+Space).
- **Leader-key gate**: the leader FSM's entry point (on the configured `leader.key`/`leader.mods`
  keypress) is now gated on `config.keybind_style == Tmux`. Under `Normal`, that keypress isn't
  intercepted at all and reaches the terminal/shell normally — no new Lua field needed, this
  falls directly out of the same `keybind_style` check.

### 5. Proposed default "normal" style bindings

Grounded in iTerm2's actual defaults where a direct equivalent exists (verified against
[iTerm2's own docs](https://iterm2.com/documentation-general-usage.html) and a
[community shortcut reference](https://gist.github.com/rannn505/0496cf92e65c69c12a8e38181dd98e6d)
rather than assumed); invented for PetruTerm-specific actions iTerm2 has no equivalent for
(AI panel, workspaces). All of this is one Lua table — trivially editable by any user after
the fact, so treat these as sensible defaults, not a locked-in contract.

| Action | Key | Action | Key |
|---|---|---|---|
| NewTab | `Cmd+T` | FixLastError | `Cmd+Option+F` |
| ClosePane | `Cmd+W` | UndoLastWrite | `Cmd+Option+Z` |
| CloseTab | `Cmd+Shift+W` | NewWorkspace | `Cmd+Shift+N` |
| NextTab / PrevTab | `Cmd+]` / `Cmd+[` | CloseWorkspace | `Cmd+Shift+X` |
| RenameTab | `Cmd+Shift+R` | RenameWorkspace | `Cmd+Option+R` |
| SplitVertical | `Cmd+D` | NextWorkspace / PrevWorkspace | `Cmd+Shift+]` / `Cmd+Shift+[` |
| SplitHorizontal | `Cmd+Shift+D` | SaveWorkspace | `Cmd+Shift+S` |
| FocusPane ←↑→↓ | `Cmd+Option+Arrows` | OpenSavedWorkspaces | `Cmd+Shift+O` |
| Resize pane* | `Cmd+Ctrl+Arrows` | ToggleSidebar | `Cmd+B` |
| ZoomPane | `Cmd+Shift+Return` | CommandPalette | `Cmd+Shift+P` |
| FocusAiPanel | `Cmd+Shift+A` | ToggleAiPanel | `Cmd+Option+A` |
| ClearAiContext | `Cmd+Option+C` | ExplainLastOutput | `Cmd+Option+E` |

\* Resize-pane is a gesture (a direct `Mux` call), not an `Action` — see below.

Counting `FocusPane`'s four directions and the two next/prev pairs individually, this table is
29 actions, matching the 14 already in `config.keys` today plus the 15 currently
hardcoded-only. 28 of them are a new `{ mods = "CMD|...", key = "...", action =
petruterm.action.X }` entry in the "normal" `config.keys` table — no new `Action` variants
needed, since these already exist and are already dispatched via `ui.handle_palette_action`
from the tmux-style hardcoded/leader paths today. The exception is resize-pane: today it's a
direct `mux.cmd_adjust_pane_ratio(dir, 0.05)` call from the Alt+Arrow branch, not routed through
`Action`/`handle_palette_action` at all — giving it a direct Cmd-combo equivalent means calling
that same `Mux` method from the new dispatch path, not adding a config.keys entry for it (there
is no `Action` variant to bind).

## Data Flow

1. `keybinds.lua` sets `config.keybind_style` and builds the matching `config.keys` table.
2. `config::lua::load_config` parses `keybind_style` into the new enum on `Config`.
3. At config-load and hot-reload, both binaries rebuild two lookup tables from `config.keys`:
   `leader_bindings_view` (unchanged, `LEADER`-only) and the new `direct_bindings_view`
   (everything else), plus the new `parse_mods`-based dispatch table for the latter.
4. On each key event: hardcoded system shortcuts first (unconditional) → direct-bind table
   lookup (unconditional) → leader FSM (only entered if `keybind_style == Tmux`).

## Error Handling

- Unrecognized `keybind_style` string in Lua → defaults to `Tmux` (same pattern as
  `TitleBarStyle`'s existing fallback, no hard error).
- Unrecognized mods token inside `parse_mods` (e.g. a typo) → that token is ignored, not a
  parse error; matches the existing permissive style of `KeyBind.mods` (a free string, not a
  validated enum at the Lua boundary).
- A direct-bind entry whose `(mods, key)` collides with an unconditional hardcoded shortcut
  (Cmd+C/V/Q/K/F/1-9) is simply never reachable — the hardcoded check runs first and returns
  early, same precedence rule that already exists between hardcoded shortcuts and the leader
  system today.

## Testing Strategy

- `direct_bindings_view` unit tests mirroring `leader_bindings_view`'s existing ones
  (`keybind_view.rs`): filters to non-`LEADER` mods, case-insensitive, empty on an all-leader
  table.
- `parse_mods` unit tests: single modifier, combined (`"CMD|SHIFT"`), unknown token ignored,
  order-independent (`"SHIFT|CMD"` == `"CMD|SHIFT"`).
- Regression tests confirming `keybind_style = "tmux"` (default) produces byte-identical
  `leader_bindings_view` output to today, on the existing default config.
- No rendering/hit-testing/dispatch-loop tests (per this project's established testing
  convention — business logic only, dogfooded manually for input handling).

## Rollout and Follow-up

1. Schema + `keybind_view` changes first (pure data layer, both binaries unaffected until wired).
2. Wire wgpu's direct-bind dispatch + leader-gate change; dogfood tmux style still works
   unchanged, then dogfood normal style manually.
3. Wire gpui's direct-bind dispatch + leader-gate change; same dogfood pass.
4. Update `keybinds.lua`'s default content (both tables) and bump its
   `-- petruterm-config-version` tag — this will overwrite any user's already-customized
   `keybinds.lua` on next launch, per the existing managed-file mechanism; call this out
   explicitly in the PR/commit message since it's a real behavior change for existing users
   even though the mechanism itself isn't new.
5. Update `AGENTS.md`'s keybind table and the in-file `keybinds.lua` header comment to document
   both styles.

## Success Criteria

- `keybind_style = "tmux"` (default) is indistinguishable from today's behavior — every
  existing leader binding and hardcoded sub-leader sequence works exactly as before.
- `keybind_style = "normal"` gives direct-key access to all 26 actions above, in both binaries,
  with the leader key fully inert (passes through to the shell).
- No regressions in the existing 152+ lib test suite; new tests cover the data layer
  (`direct_bindings_view`, `parse_mods`) at the same rigor as the existing `leader_bindings_view`
  tests.
