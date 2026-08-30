# gpui Chrome Migration — Design

**Status:** Approved, pending spec review
**Date:** 2026-08-30
**Origin:** User cloned cmux, a native macOS Swift/AppKit app (uses `libghostty` for terminal rendering, with vertical tabs and a styled sidebar), and wants PetruTerm's chrome to reach the same visual polish, while staying in Rust.

## Problem

PetruTerm's chrome — both sidebars, tabs, status bar, palette, context menu, AI panel — is hand-drawn as raw wgpu GPU instances in `src/app/renderer/*` (~7,300 lines) and `src/ui/*` (~1,600 lines), with manual hit-testing for every interactive element. This ceiling is visual polish and styling/theming reach: animations, native-feeling motion, and richer declarative styling are expensive to hand-roll in an immediate-mode-style quad-pushing pipeline. Phase 9 (floating cards, macOS blur) pushed this pipeline about as far as it reasonably goes; going further means fighting the model, not extending it.

cmux gets this by *not* implementing its own terminal renderer or chrome primitives — it embeds `libghostty` for the terminal and builds all chrome in a native retained-mode toolkit (AppKit/SwiftUI). PetruTerm's equivalent, staying in Rust: keep `alacritty_terminal` as the grid/PTY engine (unchanged, per project convention) and adopt **gpui** — the GPU-native retained-mode Rust UI framework built by the Zed team, which already solves this exact shape of problem (Zed's own terminal panel wraps `alacritty_terminal` as a custom gpui element).

## Guiding principles

These constrain every section below and every implementation decision that follows from this spec:

- **Don't reinvent what gpui already provides.** Prefer gpui's native keybinding/chord dispatch, focus system, layout, and animation primitives over hand-rolled equivalents. Custom code is justified only where gpui doesn't cover the need (e.g. terminal-grid painting).
- **Least code that solves the problem.** No speculative abstraction, no infrastructure built ahead of a concrete need.
- **Tests are for business logic, not rendering.** Pane/tab/workspace operations, leader-key dispatch (if it stays custom), and any surviving pure-logic modules get unit tests. GPU painting and layout are verified by dogfooding, not by a test harness built to simulate them.

## Scope

The whole chrome layer: both sidebars (workspace nav, AI chat), tabs, status bar, command palette, context menu, search bar, info overlay/toasts. Terminal grid *rendering* changes host (from wgpu to gpui's paint context) but not model — `alacritty_terminal`, `Mux`, `Workspace`, the Lua config DSL, and the LLM engine are unaffected; none of them touch rendering today.

Out of scope: any change to terminal semantics, PTY handling, config schema, or LLM providers.

## Architecture

gpui owns the window, GPU device (its own Metal backend on macOS — wgpu is retired for the shell), input delivery, layout, and the retained-element tree. `alacritty_terminal` keeps owning the grid/PTY exactly as today.

A new `TerminalGridElement` (custom gpui `Element` impl) is the one seam between old and new: it paints the grid, cursor, and selection, and forwards focused key/mouse events into the same input-dispatch logic that exists today. Every other surface (`sidebar.rs`, `tabs.rs`, `panes.rs`, `context_menu.rs`, `status_bar.rs`, chat/AI panel, palette) is rebuilt as real gpui views, replacing hand-rolled draw+hit-test code.

Rewrite is scoped to `src/app/renderer/*`, `src/ui/*`, the event-loop/input-dispatch portion of `src/app/mod.rs`, and `src/font/*` (pending the text-rendering decision below).

## Terminal grid text rendering

Highest-risk, most consequential decision, resolved by default rather than left open:

**Default: keep `cosmic-text` shaping, gpui only paints.** `TerminalGridElement::paint()` blits the already-rasterized glyph atlas (from PetruTerm's existing `cosmic-text` + `freetype`/`swash` pipeline) into gpui's paint context as textured quads — the same shape as `iced::Shader` or `egui`'s paint callbacks. `alacritty_terminal`'s grid model and cell→glyph mapping are untouched; only the compositing target changes.

This is a deliberate choice, not an open question to explore at implementation time. Reasoning: Zed's own terminal panel (built on gpui, by the gpui team) has known ligature-rendering problems even with `calt`/`liga` correctly configured — checked against the user's own Zed config, which has both set correctly, ruling out user error. Zed's *editor buffer* (not its terminal) is the component that shapes ligatures reliably, which points at gpui's native text path being weaker specifically for monospace-grid/terminal contexts — exactly the component PetruTerm is building. Reusing `cosmic-text`, which already renders ligatures correctly in PetruTerm today, avoids the failure class entirely rather than risking rediscovering it.

Only fall back to gpui's native text/glyph-atlas path if M0 (below) finds the cosmic-text-shapes/gpui-paints seam doesn't work at all — not as a first choice.

## Input handling

**Keyboard / leader-key:** default to gpui's native chorded keybinding/action dispatch (the mechanism behind Zed's own `cmd-k cmd-s`-style multi-key bindings) to express the leader-key model (`Ctrl+F` → 1000ms timeout → `h/j/k/l`/`%`/`"`/`&`/etc.), per the "don't reinvent" principle — confirm during M0 that gpui's chord matcher covers the leader-key shape (prefix + timeout + context-scoped follow-up, e.g. sidebar-active vs. terminal-active bindings differing). Fall back to porting the existing hand-rolled timeout state machine (`handle_keyboard`/`handle_sidebar_key` in `src/app/mod.rs`) only if gpui's native chords can't express that shape. Either way, *what* a keystroke does is a straight port of existing `Mux`/`Workspace` calls — no shortcut is lost, only the delivery mechanism changes.

**Mouse / hit-testing:** gpui's layout tree does hit-testing natively. Manual pixel-math code (`separator_at_pixel()`, `clamp_sidebar_cursor()` in `src/app/mod.rs`) gets deleted, not ported — a resize handle becomes a small view with its own drag handlers.

**Focus routing** (terminal pane vs. sidebar vs. AI panel): moves onto gpui's `FocusHandle` in place of hand-tracked focus state.

## Repaint reliability

PetruTerm previously fought a real bug in this class on winit/macOS: PTY output arriving without triggering a repaint until an unrelated event nudged it (paste/atuin invisible until scroll — `gotcha_lost_pty_echo_wakeup` memory, fixed via a grace-window wakeup). Separately, Zed itself has a known bug where a space typed in vi-mode doesn't render until the next keystroke — plausibly the same failure class, in Zed's app-level vim-emulation code rather than gpui itself (if it were framework-wide, Zed would be unusable broadly, not broken in one specific mode).

Given prior first-hand experience with this exact failure mode, M0 and M1 (below) include an explicit manual check: send PTY output / a keystroke, wait, observe, with zero incidental events (no scroll, no resize, no window focus change) — confirm the frame updates on its own. This is a dogfood step, not automated test infrastructure.

## Migration & isolation strategy

**Isolation:** a dedicated git worktree, not just a branch — e.g. `../PetruTerm-gpui` on a `gpui-migration` branch. `master` stays untouched and buildable throughout; the current terminal keeps working in the main checkout regardless of migration state. Nothing merges to `master` until the new shell reaches parity and has seen real use.

**Milestones**, each an independent go/no-go checkpoint:

1. **M0 — Foundation spike.** gpui window running; `TerminalGridElement` painting the grid via cosmic-text-shapes/gpui-paints; leader-key dispatch wired for one real action (e.g. split); manual repaint-reliability check. Determines whether M1+ proceeds as designed or the text-rendering/keybinding fallbacks are needed.
2. **M1 — Grid at parity.** Cursor, selection, scrollback, ligatures, emoji, LCD subpixel AA matching `master`. Chrome can be minimal/placeholder — bar is "the terminal itself is not a regression," verified by dogfood + the repaint check.
3. **M2 — Core chrome.** Tabs, status bar, pane splits/resize/zoom — daily-use surfaces needed before the branch is dogfoodable at all.
4. **M3 — Sidebars.** Workspace nav + AI chat panel, with real gpui styling/animation — the feature that motivated this migration.
5. **M4 — Remaining surfaces.** Command palette, context menu, search bar, info overlay/toasts.
6. **M5 — Cleanup & merge.** Delete the old wgpu/winit renderer and `src/ui/*` draw code; drop now-unused deps (`freetype`/`font-kit`) only if M0 ends up choosing gpui's native text path instead of the cosmic-text default; merge to `master`.

Each milestone must be independently testable/dogfoodable before the next starts.

## Testing & verification

Scoped tightly, per the guiding principles — no infrastructure built to simulate GPU rendering:

- **Unit tests, business logic only:** pane/tab/workspace operations in `Mux`/`Workspace` (unaffected by this migration, but any logic touched in the process gets covered); leader-key/chord dispatch logic, only if it ends up custom (i.e., only if gpui's native chords don't cover the leader-key shape and the fallback state machine is built).
- **No tests for painting, layout, or hit-testing** — these are verified by dogfooding each milestone, same as Phase 9's practice (GPU windows can't be captured/verified from the agent sandbox).
- **Repaint-reliability** is a manual dogfood step at M0/M1, not automated harness (see above).
- **Keybind regression checklist:** before M5 merges, every entry in AGENTS.md's keybind table is manually exercised against the gpui build.
- **Standard gates apply before the M5 merge:** `cargo clippy`, `cargo fmt`, `scripts/ci-local.sh`.

## Explicitly out of scope / not being built

- No custom hit-testing framework — gpui's layout tree replaces it.
- No custom animation/easing system — gpui's built-in transition primitives are used as-is unless a real gap is found.
- No headless PTY test harness for repaint verification — manual dogfood only.
- No change to `alacritty_terminal` grid/PTY model, Lua config DSL, or LLM engine.
