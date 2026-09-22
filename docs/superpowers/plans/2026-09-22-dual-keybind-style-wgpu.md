# Dual Keybind Style (Data Layer + wgpu Binary) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `config.keybind_style = "tmux" | "normal"` property (default `"tmux"`) and make
`"normal"` fully usable in the `petruterm` (wgpu) binary: every action reachable via the leader
system today gets a direct macOS Cmd-combo equivalent, with the leader key itself disabled when
`"normal"` is active.

**Architecture:** One new `keybind_view` layer (`Mods` parser + `direct_bindings_view`) shared by
both binaries, feeding a new direct-bind dispatch table in `InputHandler` alongside the existing
`leader_map`. `keybinds.lua` becomes one file holding both `config.keys` tables, switching on its
own `keybind_style` property — not two files (`config.lua`'s module chain calls fixed module
names, it can't pick between two files by that property). Arrow-key and Return-key bindings
(resize, pane-focus) stay hardcoded exactly like the existing tmux-style resize gesture already
is — `Key::Named` isn't representable in the current `KeyBind.key` string model at all, in either
style.

**Tech Stack:** Rust, mlua (Lua 5.4 config DSL), winit (key events, wgpu binary only).

**Spec:** `docs/superpowers/specs/2026-09-22-dual-keybind-style-design.md`

## Global Constraints

- Default `keybind_style` is `"tmux"` — must be byte-identical to current behavior when unset.
- `keybind_style = "normal"` must give every one of the 29 actions enumerated in the spec a
  direct-key path (23 as data-driven `config.keys` entries in this plan's scope; sidebar-toggle,
  resize-pane, and the 4 pane-focus directions are hardcoded, matching existing precedent — see
  Task 6).
- No behavior change to `keybind_style = "tmux"` (the leader FSM, `leader_map`, and every
  hardcoded sub-leader sequence in `src/app/input/mod.rs` are untouched).
- This plan covers `src/config/` (shared) and the wgpu binary only. gpui wiring is a separate
  follow-up plan — `gpui_shell`'s `LeaderAction` enum (`src/gpui_shell/leader.rs`) is missing
  `ClearAiContext`, `SaveWorkspace`, and `OpenSavedWorkspaces` entirely, a pre-existing gap
  unrelated to this feature that needs its own fix first.

---

## Corrections made to the spec during planning

Two things the spec got slightly wrong, found while reading the real dispatch code:

1. **`petruterm.action` is a curated Lua whitelist, not exhaustive** (`src/config/lua.rs:349-377`).
   It's missing `ZoomPane`, `ClearAiContext`, `NewWorkspace`, `CloseWorkspace`,
   `RenameWorkspace`, `NextWorkspace`, `PrevWorkspace`, `SaveWorkspace`, `OpenSavedWorkspaces`.
   Without adding these, `petruterm.action.ZoomPane` etc. would be `nil` in Lua, and the
   resulting `config.keys` entry would silently vanish (`action` parses to `""`, which
   `lua.rs:599`'s `!action.is_empty()` check drops without error). Task 1 fixes this.
2. **Arrow keys and Return aren't just "the resize gesture's problem"** — `event.logical_key`
   for any of them is `Key::Named`, never `Key::Character`, and `direct_bindings_view`'s
   `KeyBind.key` is a plain string matched only against `Key::Character` (same as the existing
   `leader_map`). So pane-focus (`Cmd+Option+Arrow`) needs the same hardcoded treatment as
   resize-pane, not a `config.keys` entry — Task 6 handles both together. `ZoomPane`'s proposed
   key also moved from `Cmd+Shift+Return` to `Cmd+Shift+M` (mnemonic "maximize") for this same
   reason, staying data-driven instead of adding a third hardcoded special case.

---

### Task 1: `KeybindStyle` config schema + Lua parsing + action whitelist

**Files:**
- Modify: `src/config/schema.rs` (add enum, `Config` field, `Config::default()`)
- Modify: `src/config/lua.rs` (parse the property, extend the action whitelist)
- Test: inline `#[cfg(test)]` in `src/config/schema.rs`

**Interfaces:**
- Produces: `pub enum KeybindStyle { Tmux, Normal }` (Default = `Tmux`), `Config.keybind_style:
  KeybindStyle`, both consumed by Tasks 6/7 as `crate::config::schema::KeybindStyle`.

- [ ] **Step 1: Add the `KeybindStyle` enum to `schema.rs`**

Add right after `LeaderConfig`'s `impl Default` block (currently ending around line 555, next to
`SnippetConfig`):

```rust
/// Which keybinding scheme is active: the default tmux-style leader-key
/// scheme, or direct macOS Cmd-combos for users unfamiliar with tmux
/// conventions. Set via `config.keybind_style` in `keybinds.lua`, which
/// owns both `config.keys` tables and switches between them on this same
/// property -- see `config/default/keybinds.lua`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeybindStyle {
    #[default]
    Tmux,
    Normal,
}
```

- [ ] **Step 2: Add the field to `Config` and its default**

In the `Config` struct (around line 14), add right after `pub keys: Vec<KeyBind>,`:

```rust
    pub keybind_style: KeybindStyle,
```

In `impl Default for Config`'s body (around line 96), add right after `keys: vec![],`:

```rust
            keybind_style: KeybindStyle::default(),
```

- [ ] **Step 3: Write the failing test**

Add to `schema.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn keybind_style_defaults_to_tmux() {
    assert_eq!(Config::default().keybind_style, KeybindStyle::Tmux);
}
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test --bin petruterm keybind_style_defaults_to_tmux`
Expected: FAIL — `keybind_style` field/`KeybindStyle` type don't exist yet (compile error) unless
Steps 1-2 are already done; if you did Steps 1-2 first, this instead passes immediately — that's
fine, the next step confirms parsing.

- [ ] **Step 5: Parse `config.keybind_style` from Lua**

In `src/config/lua.rs`, add right after the `keys` table block (after line 605's closing `}`,
before the `llm` table block):

```rust
    if let Ok(style) = table.get::<String>("keybind_style") {
        config.keybind_style = match style.as_str() {
            "normal" | "Normal" => super::schema::KeybindStyle::Normal,
            _ => super::schema::KeybindStyle::Tmux,
        };
    }
```

- [ ] **Step 6: Extend the `petruterm.action` whitelist**

In `src/config/lua.rs`, add these 9 names to the `for name in &[...]` list at line 350 (any
position in the list is fine — it's just a lookup table):

```rust
        "ZoomPane",
        "ClearAiContext",
        "NewWorkspace",
        "CloseWorkspace",
        "RenameWorkspace",
        "NextWorkspace",
        "PrevWorkspace",
        "SaveWorkspace",
        "OpenSavedWorkspaces",
```

- [ ] **Step 7: Run the full config test suite**

Run: `cargo test --bin petruterm config::`
Expected: PASS, including the new `keybind_style_defaults_to_tmux` test and every existing
config test (nothing here changes existing behavior).

- [ ] **Step 8: Commit**

```bash
git add src/config/schema.rs src/config/lua.rs
git commit -m "feat: Add keybind_style config property and extend action whitelist."
```

---

### Task 2: `Mods` parser in `keybind_view.rs`

**Files:**
- Modify: `src/config/keybind_view.rs`

**Interfaces:**
- Consumes: nothing new (pure string parsing).
- Produces: `pub struct Mods { pub cmd: bool, pub shift: bool, pub ctrl: bool, pub option: bool }`
  (derives `Debug, Clone, Copy, PartialEq, Eq, Hash, Default`), `pub fn parse_mods(s: &str) ->
  Mods`. Task 4 uses `Mods` as half of `DirectBindingsView` consumers' dispatch-table key; Task 6
  builds `InputHandler.direct_map: HashMap<(Mods, String), Action>` from it.

- [ ] **Step 1: Write the failing tests**

Add to `keybind_view.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn parse_mods_single_token() {
    let m = parse_mods("CMD");
    assert!(m.cmd);
    assert!(!m.shift && !m.ctrl && !m.option);
}

#[test]
fn parse_mods_combined_tokens() {
    let m = parse_mods("CMD|SHIFT");
    assert!(m.cmd);
    assert!(m.shift);
    assert!(!m.ctrl && !m.option);
}

#[test]
fn parse_mods_case_insensitive() {
    assert_eq!(parse_mods("cmd|shift"), parse_mods("CMD|SHIFT"));
}

#[test]
fn parse_mods_unknown_token_ignored() {
    let m = parse_mods("CMD|BOGUS");
    assert!(m.cmd);
    assert_eq!(m, parse_mods("CMD"));
}

#[test]
fn parse_mods_empty_string_is_no_modifiers() {
    assert_eq!(parse_mods(""), Mods::default());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --bin petruterm parse_mods`
Expected: FAIL — `Mods`/`parse_mods` don't exist yet.

- [ ] **Step 3: Implement `Mods` and `parse_mods`**

Add above `LeaderBindingsView` in `keybind_view.rs`:

```rust
/// A parsed modifier set from a `KeyBind.mods` string like `"CMD|SHIFT"`.
/// Case-insensitive, `|`-separated; an unrecognized token is ignored,
/// matching the permissive style of the rest of `KeyBind` parsing at the
/// Lua boundary (`lua.rs`'s own `unwrap_or_default()` reads).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Mods {
    pub cmd: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub option: bool,
}

pub fn parse_mods(s: &str) -> Mods {
    let mut mods = Mods::default();
    for token in s.split('|') {
        match token.trim().to_ascii_uppercase().as_str() {
            "CMD" => mods.cmd = true,
            "SHIFT" => mods.shift = true,
            "CTRL" => mods.ctrl = true,
            "OPTION" => mods.option = true,
            _ => {}
        }
    }
    mods
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --bin petruterm parse_mods`
Expected: PASS (all 5 tests).

- [ ] **Step 5: Commit**

```bash
git add src/config/keybind_view.rs
git commit -m "feat: Add Mods modifier-string parser to keybind_view."
```

---

### Task 3: `direct_bindings_view` in `keybind_view.rs`

**Files:**
- Modify: `src/config/keybind_view.rs`

**Interfaces:**
- Consumes: `Config.keys: Vec<KeyBind>` (existing).
- Produces: `pub struct DirectBindingsView { pub bindings: Vec<KeyBind> }`, `pub fn
  direct_bindings_view(config: &Config) -> DirectBindingsView`. Task 6 calls this the same way
  `InputHandler::new` already calls `leader_bindings_view`.

- [ ] **Step 1: Write the failing tests**

Add to the same test module (reuse the existing `kb()` helper already defined there):

```rust
#[test]
fn direct_bindings_view_filters_out_leader_case_insensitive() {
    let config = Config {
        keys: vec![
            kb("LEADER", "c", "NewTab"),
            kb("CMD", "t", "NewTab"),
            kb("CMD|SHIFT", "w", "CloseTab"),
            kb("leader", "x", "ClosePane"),
        ],
        ..Config::default()
    };
    let view = direct_bindings_view(&config);
    assert_eq!(view.bindings.len(), 2);
    assert!(view
        .bindings
        .iter()
        .any(|kb| kb.mods == "CMD" && kb.key == "t"));
    assert!(view
        .bindings
        .iter()
        .any(|kb| kb.mods == "CMD|SHIFT" && kb.key == "w"));
}

#[test]
fn direct_bindings_view_empty_when_all_leader() {
    let config = Config {
        keys: vec![kb("LEADER", "c", "NewTab")],
        ..Config::default()
    };
    assert!(direct_bindings_view(&config).bindings.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --bin petruterm direct_bindings_view`
Expected: FAIL — `DirectBindingsView`/`direct_bindings_view` don't exist yet.

- [ ] **Step 3: Implement `DirectBindingsView` and `direct_bindings_view`**

Add below `leader_bindings_view` in `keybind_view.rs`:

```rust
#[derive(Debug, Clone)]
pub struct DirectBindingsView {
    pub bindings: Vec<KeyBind>,
}

/// Every `config.keys` entry whose `mods` is NOT `"LEADER"` -- the direct
/// (non-leader) keybind scheme used when `keybind_style = "normal"`.
/// Mirrors `leader_bindings_view`'s own filter, inverted.
pub fn direct_bindings_view(config: &Config) -> DirectBindingsView {
    DirectBindingsView {
        bindings: config
            .keys
            .iter()
            .filter(|kb| !kb.mods.eq_ignore_ascii_case("LEADER"))
            .cloned()
            .collect(),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --bin petruterm direct_bindings_view`
Expected: PASS (both tests).

- [ ] **Step 5: Run the full `keybind_view` test suite for regressions**

Run: `cargo test --bin petruterm keybind_view::`
Expected: PASS — every existing `leader_bindings_view` test plus Task 2's and this task's new
ones.

- [ ] **Step 6: Commit**

```bash
git add src/config/keybind_view.rs
git commit -m "feat: Add direct_bindings_view to keybind_view."
```

---

### Task 4: `keybinds.lua` — both tables, style switch, version bump

**Files:**
- Modify: `config/default/keybinds.lua` (full rewrite)
- Test: `src/config/mod.rs`'s existing `#[cfg(test)] mod tests` block

**Interfaces:**
- Consumes: `petruterm.action.*` (Task 1's extended whitelist), `config.keybind_style` (Task 1).
- Produces: the real default content Task 6/7's dispatch tables are built from at runtime, and
  what the embedded-config regression test (Step 3 below) exercises through the real Lua VM.

- [ ] **Step 1: Rewrite `config/default/keybinds.lua`**

`keybinds.lua` is a *managed* file (`update_managed_configs`, `src/config/mod.rs`): PetruTerm
silently overwrites a user's already-installed `~/.config/petruterm/keybinds.lua` with the
bundled default whenever the `-- petruterm-config-version:` tag bumps, on next launch. Bumping
it here (3 → 4, in the replacement below) means this commit resets every existing user's
keybind customizations back to this new default the next time they launch. That's the existing
mechanism working as designed, not a new bug — call it out explicitly in this task's commit
message so it's not a surprise in review. Replace the whole file:

```lua
-- PetruTerm keybinds configuration
-- petruterm-config-version: 4
-- keybind_style = "tmux" (default): Leader key Ctrl+F, 1000ms timeout.
--   After pressing Ctrl+F, press the bound key within the timeout window.
-- keybind_style = "normal": direct macOS Cmd-combos, no leader key at all
--   (Ctrl+F passes through to the shell instead of activating a leader).
--
-- System keybinds that remain hardcoded regardless of style (not configurable here):
--   Cmd+C / Cmd+V   — copy / paste (clipboard)
--   Cmd+Q           — quit
--   Cmd+K           — clear screen and scrollback
--   Cmd+F           — open / close text search (Enter=next, Shift+Enter=prev, Esc=close)
--   Cmd+1-9         — switch to tab N
--
-- "normal" style keybinds that are ALSO hardcoded, not config.keys entries below:
-- arrow-key and Return-key presses arrive as named keys, not characters, and
-- KeyBind.key only ever matches a character (same reason the resize gesture
-- below is hardcoded even in tmux style).
--   Cmd+B             — toggle sidebar
--   Cmd+Option+Arrows — focus pane left/right/up/down
--   Cmd+Ctrl+Arrows   — resize focused pane

local petruterm = require("petruterm")
local module    = {}

function module.apply_to_config(config)
  config.keybind_style = "tmux"  -- "tmux" | "normal"
  config.leader = { key = "f", mods = "CTRL", timeout_ms = 1000 }

  -- Keyboard options.
  -- option_as_meta = false (default): Option/Alt acts as a compose key.
  --   Characters like {, }, @, # produced via Option on non-US keyboards work correctly.
  -- option_as_meta = true: Option/Alt sends ESC prefix (Meta key for Emacs/readline).
  config.keyboard = { option_as_meta = false }

  if config.keybind_style == "tmux" then
    config.keys = {
      -- ── Overlays ──────────────────────────────────────────────────────────
      { mods = "LEADER", key = "o",  action = petruterm.action.CommandPalette },

      -- ── AI controls ────────────────────────────────────────────────────────
      -- leader+A   : focus AI panel / return focus to terminal
      { mods = "LEADER", key = "A",  action = petruterm.action.FocusAiPanel },
      -- leader+a+a : toggle AI panel open / close
      -- leader+a+e : Explain last output
      -- leader+a+f : Fix last error
      -- leader+a+z : Undo last write
      -- (These are handled as hardcoded sub-leader sequences, not config entries.)

      -- ── Explorer sub-leader (leader+e+*) ──────────────────────────────────
      -- leader+e+e : Toggle workspace sidebar

      -- ── Tabs (tmux-style) ─────────────────────────────────────────────────
      { mods = "LEADER", key = "c",  action = petruterm.action.NewTab },
      { mods = "LEADER", key = "&",  action = petruterm.action.CloseTab },
      { mods = "LEADER", key = "n",  action = petruterm.action.NextTab },
      { mods = "LEADER", key = "b",  action = petruterm.action.PrevTab },
      { mods = "LEADER", key = ",",  action = petruterm.action.RenameTab },

      -- ── Pane splits (tmux-style) ───────────────────────────────────────────
      { mods = "LEADER", key = "%",  action = petruterm.action.SplitHorizontal },
      { mods = "LEADER", key = '"',  action = petruterm.action.SplitVertical },
      { mods = "LEADER", key = "x",  action = petruterm.action.ClosePane },

      -- ── Pane focus (vim-style) ─────────────────────────────────────────────
      { mods = "LEADER", key = "h",  action = petruterm.action.FocusPaneLeft },
      { mods = "LEADER", key = "j",  action = petruterm.action.FocusPaneDown },
      { mods = "LEADER", key = "k",  action = petruterm.action.FocusPaneUp },
      { mods = "LEADER", key = "l",  action = petruterm.action.FocusPaneRight },
    }
  else
    config.keys = {
      -- ── Tabs ────────────────────────────────────────────────────────────
      { mods = "CMD",         key = "t", action = petruterm.action.NewTab },
      { mods = "CMD|SHIFT",   key = "w", action = petruterm.action.CloseTab },
      { mods = "CMD",         key = "]", action = petruterm.action.NextTab },
      { mods = "CMD",         key = "[", action = petruterm.action.PrevTab },
      { mods = "CMD|SHIFT",   key = "r", action = petruterm.action.RenameTab },

      -- ── Panes ───────────────────────────────────────────────────────────
      { mods = "CMD",         key = "w", action = petruterm.action.ClosePane },
      { mods = "CMD",         key = "d", action = petruterm.action.SplitVertical },
      { mods = "CMD|SHIFT",   key = "d", action = petruterm.action.SplitHorizontal },
      { mods = "CMD|SHIFT",   key = "m", action = petruterm.action.ZoomPane },

      -- ── Overlays ────────────────────────────────────────────────────────
      { mods = "CMD|SHIFT",   key = "p", action = petruterm.action.CommandPalette },

      -- ── AI controls ─────────────────────────────────────────────────────
      { mods = "CMD|SHIFT",   key = "a", action = petruterm.action.FocusAiPanel },
      { mods = "CMD|OPTION",  key = "a", action = petruterm.action.ToggleAiPanel },
      { mods = "CMD|OPTION",  key = "c", action = petruterm.action.ClearAiContext },
      { mods = "CMD|OPTION",  key = "e", action = petruterm.action.ExplainLastOutput },
      { mods = "CMD|OPTION",  key = "f", action = petruterm.action.FixLastError },
      { mods = "CMD|OPTION",  key = "z", action = petruterm.action.UndoLastWrite },

      -- ── Workspaces ──────────────────────────────────────────────────────
      { mods = "CMD|SHIFT",   key = "n", action = petruterm.action.NewWorkspace },
      { mods = "CMD|SHIFT",   key = "x", action = petruterm.action.CloseWorkspace },
      { mods = "CMD|OPTION",  key = "r", action = petruterm.action.RenameWorkspace },
      { mods = "CMD|SHIFT",   key = "]", action = petruterm.action.NextWorkspace },
      { mods = "CMD|SHIFT",   key = "[", action = petruterm.action.PrevWorkspace },
      { mods = "CMD|SHIFT",   key = "s", action = petruterm.action.SaveWorkspace },
      { mods = "CMD|SHIFT",   key = "o", action = petruterm.action.OpenSavedWorkspaces },
    }
  end
end

return module
```

- [ ] **Step 2: Write the failing regression test**

`config/default/config.lua` (`DEFAULT_CONFIG`) ends with `return config` — Lua requires `return`
to be a block's last statement, so a test can't just append more source after it. Instead,
override the *module* `config.lua` requires: `EMBEDDED_MODULES` (`src/config/mod.rs:33`) is the
`(name, source)` list `load_config_str`'s `preloaded` param resolves `require(...)` against
during a test load (no filesystem access) — swap in a copy of `DEFAULT_KEYBINDS`
(`src/config/mod.rs:17`, private but same-module-visible to this test) with its one
`"tmux"` replaced, keeping every other module (`ui`, `perf`, `llm`, `snippets`, `notifications`,
`petruterm`) exactly as shipped. Add to `src/config/mod.rs`'s `#[cfg(test)] mod tests` block
(same style as `embedded_default_config_sets_agent_backend`):

```rust
#[test]
fn embedded_default_keybinds_normal_style_resolves_every_action() {
    // Overrides keybinds.lua's shipped `config.keybind_style = "tmux"` line
    // the same way a user editing their own copy of the file would, then
    // loads the real config.lua through the real Lua VM -- proving every
    // petruterm.action.* name the "normal" table uses is both in the
    // whitelist (lua.rs's `action.set` list) and a real Action::from_str
    // variant, not just present in the .lua source file.
    let normal_keybinds = DEFAULT_KEYBINDS.replacen(
        "config.keybind_style = \"tmux\"",
        "config.keybind_style = \"normal\"",
        1,
    );
    let mut preloaded: Vec<(&str, &str)> = EMBEDDED_MODULES
        .iter()
        .filter(|(name, _)| *name != "keybinds")
        .cloned()
        .collect();
    preloaded.push(("keybinds", normal_keybinds.as_str()));

    let (config, _lua) =
        lua::load_config_str(DEFAULT_CONFIG, "default/config.lua", &preloaded)
            .expect("embedded default config with keybind_style=normal must load");
    assert_eq!(config.keybind_style, schema::KeybindStyle::Normal);
    assert_eq!(config.keys.len(), 23);
    assert!(config
        .keys
        .iter()
        .all(|kb| kb.action.parse::<crate::ui::palette::Action>().is_ok()));
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --bin petruterm embedded_default_keybinds_normal_style_resolves_every_action`
Expected: FAIL — `keybind_style` doesn't exist/parse yet unless Task 1 already landed, or the
`.replacen` finds no match yet if Step 1 (this task) hasn't landed either. If both Task 1 and
this task's Step 1 are already done before writing this test, it should pass immediately —
that's fine, it's still a real regression guard from here on.

- [ ] **Step 4: Fix `keybinds.lua`/whitelist until the test passes**

Run: `cargo test --bin petruterm embedded_default_keybinds_normal_style_resolves_every_action`
Expected: PASS. If any action fails to parse, it's either missing from Task 1's whitelist
extension or misspelled in Step 1's Lua table — fix at the source (whitelist or Lua), not by
weakening the test.

- [ ] **Step 5: Run the full existing config test suite for regressions**

Run: `cargo test --bin petruterm config::`
Expected: PASS — `embedded_default_config_sets_agent_backend` and every other existing test in
this module still pass; the "tmux" default table is byte-identical to before.

- [ ] **Step 6: Commit**

```bash
git add config/default/keybinds.lua src/config/mod.rs
git commit -m "$(cat <<'EOF'
feat: Add normal keybind style table to keybinds.lua.

Bumps keybinds.lua's petruterm-config-version 3 -> 4. This is a
*managed* config file: bumping it resets every existing user's
already-installed ~/.config/petruterm/keybinds.lua to this new default
on next launch (update_managed_configs, src/config/mod.rs) -- existing
mechanism, not new, but real behavior change for anyone who customized
config.keys before this commit.
EOF
)"
```

---

### Task 5: wgpu `InputHandler` — direct-bind dispatch table

**Files:**
- Modify: `src/app/input/mod.rs`

**Interfaces:**
- Consumes: `config::keybind_view::{direct_bindings_view, Mods, parse_mods}` (Tasks 2-3),
  `Action: FromStr` (existing, `src/ui/palette/actions.rs`).
- Produces: `InputHandler.direct_map: HashMap<(Mods, String), Action>`, built once in `new()` the
  same way `leader_map` already is. Task 6 reads this field.

- [ ] **Step 1: Add the field**

In `InputHandler`'s struct definition (around line 29, right after `pub leader_map:
HashMap<String, Action>,`):

```rust
    /// Maps (modifier set, character) → Action for "normal" keybind style,
    /// built from `config.keys`'s non-LEADER bindings. Empty (and inert)
    /// under "tmux" style, since keybinds.lua's tmux table has no non-LEADER
    /// entries.
    pub direct_map: HashMap<(crate::config::keybind_view::Mods, String), Action>,
```

- [ ] **Step 2: Build it in `InputHandler::new`**

Right after the existing `leader_map` construction (around line 85, after the `.collect();`
closing the `leader_map` binding, before the `Self { ... }` struct literal), add:

```rust
        let direct_view = crate::config::keybind_view::direct_bindings_view(config);
        let direct_map = direct_view
            .bindings
            .iter()
            .filter_map(|kb| {
                let action = kb.action.parse::<Action>().ok()?;
                let mods = crate::config::keybind_view::parse_mods(&kb.mods);
                Some(((mods, kb.key.clone()), action))
            })
            .collect();
```

Then add `direct_map,` to the `Self { ... }` struct literal, right after `leader_map,`.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check --bin petruterm`
Expected: clean — no test yet, this is pure wiring matching an existing pattern
(`leader_map`'s own construction) one line over.

- [ ] **Step 4: Write a regression test for the builder logic**

`InputHandler::new` takes a full `Config` plus other setup that's awkward to construct in a unit
test. Instead, test the same filter/parse logic `direct_bindings_view` + `parse_mods` already
cover in Tasks 2-3 stays correct when combined — add to `keybind_view.rs`'s test module (this
belongs there, not in `input/mod.rs`, since it's data-layer logic being exercised, not anything
input-handling-specific):

```rust
#[test]
fn direct_bindings_view_output_parses_into_real_actions() {
    // Exercises the exact same two-step pipeline InputHandler::new runs
    // (direct_bindings_view -> parse mods + action per binding), using a
    // real Action string from this codebase rather than a placeholder, to
    // catch a future Action rename that keybind_view itself has no direct
    // dependency on.
    let config = Config {
        keys: vec![kb("CMD|SHIFT", "w", "CloseTab")],
        ..Config::default()
    };
    let view = direct_bindings_view(&config);
    let kb = &view.bindings[0];
    let mods = parse_mods(&kb.mods);
    assert!(mods.cmd && mods.shift && !mods.ctrl && !mods.option);
    assert_eq!(kb.key, "w");
}
```

This test needs `crate::ui::palette::Action` reachable from `keybind_view.rs`'s test module, but
`keybind_view.rs` itself must NOT depend on `Action` (it's wgpu-specific; `keybind_view` is
shared with `gpui_shell`, which has its own `LeaderAction` instead) — this test only checks the
`Mods`/`KeyBind` shape, not that `"CloseTab"` parses as an `Action`, keeping that separation.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --bin petruterm direct_bindings_view_output_parses_into_real_actions`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/app/input/mod.rs src/config/keybind_view.rs
git commit -m "feat: Build direct-bind dispatch table in wgpu InputHandler."
```

---

### Task 6: wgpu `handle_key_input` — dispatch wiring + leader gate

**Files:**
- Modify: `src/app/input/mod.rs`

**Interfaces:**
- Consumes: `self.direct_map` (Task 5), `config.keybind_style` (Task 1), `ui.handle_palette_action`
  (existing, same signature already used at lines 326-336 and 359-369), `mux.cmd_adjust_pane_ratio`
  (existing, same call already used at line 235), `self.toggle_sidebar_requested` (existing field).

- [ ] **Step 1: Add the direct-bind dispatch check**

In `handle_key_input`, insert right after the rename-prompt block (after line 244's closing
`}`), before the `// ── Leader key activation` comment (line 246):

```rust
        // ── Direct (non-leader) keybind dispatch — "normal" keybind style ──
        // Inert under "tmux" style: direct_map is empty there (keybinds.lua's
        // tmux table has no non-LEADER entries), so this never matches.
        if let Key::Character(s) = &event.logical_key {
            let mods = crate::config::keybind_view::Mods {
                cmd,
                shift,
                ctrl,
                option: self.modifiers.state().alt_key(),
            };
            if let Some(action) = self.direct_map.get(&(mods, s.to_string())).cloned() {
                if let Some(rc) = render_ctx.as_mut() {
                    ui.handle_palette_action(action, mux, rc, config, window, wakeup_proxy);
                }
                return;
            }
        }
```

- [ ] **Step 2: Gate leader-key activation on `keybind_style`**

Change the leader-activation check (currently line 248):

```rust
        if ctrl && !shift && !cmd {
```

to:

```rust
        if ctrl
            && !shift
            && !cmd
            && config.keybind_style == crate::config::schema::KeybindStyle::Tmux
        {
```

- [ ] **Step 3: Add the three hardcoded "normal"-only shortcuts**

Right after Step 1's new block (still before the leader-activation check), add:

```rust
        // ── Hardcoded "normal"-only shortcuts ────────────────────────────
        // Not config.keys entries: Cmd+B is a plain character but kept
        // hardcoded to match the existing tmux-style sidebar toggle
        // (`leader s` / `leader e e`), which is ALSO hardcoded, not
        // config-driven, today. Cmd+Option/Ctrl+Arrow are Key::Named, never
        // Key::Character, so they can't be config.keys entries at all (see
        // this plan's "Corrections to the spec").
        if config.keybind_style == crate::config::schema::KeybindStyle::Normal {
            let option = self.modifiers.state().alt_key();
            if cmd && !shift && !ctrl && !option {
                if let Key::Character(s) = &event.logical_key {
                    if s.as_str() == "b" {
                        self.toggle_sidebar_requested = true;
                        return;
                    }
                }
            }
            if cmd && (option || ctrl) && !shift {
                use crate::ui::panes::FocusDir;
                let dir_opt = match &event.logical_key {
                    Key::Named(NamedKey::ArrowLeft) => Some(FocusDir::Left),
                    Key::Named(NamedKey::ArrowRight) => Some(FocusDir::Right),
                    Key::Named(NamedKey::ArrowUp) => Some(FocusDir::Up),
                    Key::Named(NamedKey::ArrowDown) => Some(FocusDir::Down),
                    _ => match &event.physical_key {
                        PhysicalKey::Code(KeyCode::ArrowLeft) => Some(FocusDir::Left),
                        PhysicalKey::Code(KeyCode::ArrowRight) => Some(FocusDir::Right),
                        PhysicalKey::Code(KeyCode::ArrowUp) => Some(FocusDir::Up),
                        PhysicalKey::Code(KeyCode::ArrowDown) => Some(FocusDir::Down),
                        _ => None,
                    },
                };
                if let Some(dir) = dir_opt {
                    if option {
                        // Cmd+Option+Arrow: move focus. No config.keys
                        // equivalent exists for this today either (tmux
                        // style uses h/j/k/l, not arrows).
                        if let Some(rc) = render_ctx.as_mut() {
                            ui.handle_palette_action(
                                Action::FocusPane(dir),
                                mux,
                                rc,
                                config,
                                window,
                                wakeup_proxy,
                            );
                        }
                    } else {
                        // Cmd+Ctrl+Arrow: resize. No resize_mode tracking
                        // needed here (unlike the leader+Option+Arrow path
                        // above) -- Cmd+Ctrl held is itself the complete,
                        // repeatable trigger; there's no leader state to
                        // stay latched past.
                        mux.cmd_adjust_pane_ratio(dir, 0.05);
                        self.pane_ratio_adjusted = true;
                    }
                    return;
                }
            }
        }
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check --bin petruterm`
Expected: clean.

- [ ] **Step 5: Run clippy and the full test suite**

Run: `cargo clippy --bin petruterm -- -D warnings && cargo test --bin petruterm`
Expected: both clean — no existing test exercises live key dispatch (per this project's
established convention, dispatch/input-handling is dogfooded manually, not unit tested), so this
step's job is confirming nothing else broke, not new coverage.

- [ ] **Step 6: Manual dogfood — tmux style unchanged**

Run: `cargo build --release && ./target/release/petruterm` (or however you normally launch a
locally built binary). With the default config (`keybind_style` unset), confirm: `Ctrl+F` then
`c` opens a new tab, `Ctrl+F` then `o` opens the command palette — i.e., tmux style behaves
exactly as before this plan.

- [ ] **Step 7: Manual dogfood — normal style**

Add `config.keybind_style = "normal"` to `~/.config/petruterm/keybinds.lua` (temporarily, for
this check). Relaunch and confirm at least: `Cmd+T` opens a new tab, `Ctrl+F` does nothing
(passes through — e.g. typing `printf` in the shell after pressing `Ctrl+F` should show the
`Ctrl+F` had no special effect), `Cmd+Shift+P` opens the command palette, `Cmd+Option+Arrow`
moves pane focus, `Cmd+Ctrl+Arrow` resizes a pane (only visible with a split open), `Cmd+B`
toggles the sidebar. Revert the temporary config change afterward unless you want to keep it.

- [ ] **Step 8: Commit**

```bash
git add src/app/input/mod.rs
git commit -m "feat: Wire direct-bind dispatch and leader-gate into wgpu input handler."
```

---

### Task 7: Docs + final verification

**Files:**
- Modify: `AGENTS.md` (keybind table section)

- [ ] **Step 1: Document both styles in `AGENTS.md`**

In the `## Keybinds` section, add a short note above the existing table:

```markdown
Two keybind styles are available via `config.keybind_style` in `keybinds.lua` (default
`"tmux"`, shown below). `"normal"` gives direct macOS Cmd-combos instead — see
`config/default/keybinds.lua`'s own "normal" table for the full list.
```

- [ ] **Step 2: Run the full local CI**

Run: `./scripts/ci-local.sh`
Expected: PASS — check, test, clippy, fmt, audit all clean, same as every other change in this
project.

- [ ] **Step 3: Final commit**

```bash
git add AGENTS.md
git commit -m "docs: Document dual keybind style in AGENTS.md."
```

## Follow-up (not in this plan)

- **gpui wiring.** `src/gpui_shell/leader.rs`'s `LeaderAction` enum is missing `ClearAiContext`,
  `SaveWorkspace`, and `OpenSavedWorkspaces` — a pre-existing gap in gpui's own tmux-style parity,
  unrelated to this feature. Needs its own small fix before gpui can reach the same "normal"
  style parity this plan gives the wgpu binary. Brainstorm that as its own follow-up once this
  plan is dogfooded and merged.
