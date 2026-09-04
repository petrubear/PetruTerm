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

Highest-risk, most consequential decision. Verified against gpui's actual source (`zed-industries/zed`, `main`, fetched 2026-08-30) rather than left as an assumption:

**Default: use gpui's native `text_system()` / glyph sprite atlas, matching Zed's own precedent.** Zed's real terminal renderer (`crates/terminal_view/src/terminal_element.rs`) does not maintain a custom pre-rasterized glyph atlas — it shapes and paints text through gpui's own pipeline (`window.text_system().shape_line(...)` → `ShapedLine::paint()`, or `Window::paint_glyph()` for single pre-shaped glyphs), and hand-rolls quad painting (`Window::paint_quad()`) only for backgrounds, selection, cursor, and subcell box-drawing characters. `TerminalGridElement` follows the same structure: gpui's text system does shaping/rasterization/caching; custom code is limited to walking `alacritty_terminal`'s grid, batching same-style runs, and painting backgrounds/cursor/selection/box-drawing — per the "don't reinvent" principle, no custom glyph atlas gets built.

**Why the earlier "reuse cosmic-text to dodge gpui's text path" reasoning doesn't hold up:** Zed's terminal panel does have a real, currently open ligature bug (`zed-industries/zed#11127`, confirmed with `calt`/`liga` set correctly — matches the user's own Zed config) and a second, terminal-specific one (`#48699`) where a ligature renders at the wrong (too-narrow) width *only* in the terminal panel, not the editor buffer, using the same font. Since Zed's terminal doesn't use a custom atlas either, "avoid gpui's text path" isn't actually the mechanism that would dodge this — the pattern in #48699 (correct in the buffer, wrong in the terminal, same font) points instead at a **monospace-grid cell-width vs. variable-width shaped-ligature-glyph mismatch**, specific to terminal-grid rendering, not a defect tied to which library shapes the glyph. PetruTerm's current renderer already reconciles this correctly (ligatures render right today per AGENTS.md's feature list) — that reconciliation logic (wherever it lives in `src/app/renderer/terminal.rs`/`src/font/shaper.rs`) is what has to survive the port, applied on top of gpui's shaped output, regardless of which pipeline shapes the glyph.

**M0 must explicitly reproduce #11127/#48699's symptom** (a ligature sequence like `->` or `===`, rendered via `text_system()`) as a go/no-go check, alongside the repaint-reliability check below. Fall back to a custom `cosmic-text`-shapes/`paint_image`-blits-atlas path (the original default) only if gpui's native text pipeline can't be made to reconcile cell width with ligature glyph width — not as a first choice.

**Dependency note (verified):** `gpui` is published on crates.io (`0.2.2`), but the companion crate needed for app bootstrap, `gpui_platform` (provides `application()` / the `Platform` impl per OS), is **not** on crates.io — it must be pulled from `https://github.com/zed-industries/zed` via git. gpui is pre-1.0 with frequent breaking changes; pin an exact git `rev` for `gpui_platform` (and match `gpui`'s version to what that rev's workspace uses) rather than tracking a branch, and commit `Cargo.lock`.

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

1. **M0 — Foundation spike.** gpui window running (via `gpui_platform`, pinned git rev); `TerminalGridElement` painting the grid via gpui's native `text_system()`; leader-key dispatch wired for one real action (e.g. split) via gpui's chorded keybinding, if it covers the leader+timeout+context shape; manual repaint-reliability check; manual reproduction of the #11127/#48699 ligature-width symptom. Determines whether M1+ proceeds as designed or the text-rendering/keybinding fallbacks are needed.
2. **M1 — Grid at parity.** Cursor, selection, scrollback, ligatures, emoji, LCD subpixel AA matching `master`. Chrome can be minimal/placeholder — bar is "the terminal itself is not a regression," verified by dogfood + the repaint check.
3. **M2 — Core chrome.** Tabs, status bar, pane splits/resize/zoom — daily-use surfaces needed before the branch is dogfoodable at all.
4. **M3 — Sidebars.** Workspace nav + AI chat panel, with real gpui styling/animation — the feature that motivated this migration.
5. **M4 — Remaining surfaces.** Command palette, context menu, search bar, info overlay/toasts.
6. **M5 — Cleanup & merge.** Delete the old wgpu/winit renderer and `src/ui/*` draw code; drop now-unused deps (`cosmic-text`, `freetype`, `font-kit`) if M0's default (gpui's native text path) held — keep them only if the custom-atlas fallback was needed instead; merge to `master`.

Each milestone must be independently testable/dogfoodable before the next starts.

## Testing & verification

Scoped tightly, per the guiding principles — no infrastructure built to simulate GPU rendering:

- **Unit tests, business logic only:** pane/tab/workspace operations in `Mux`/`Workspace` (unaffected by this migration, but any logic touched in the process gets covered); leader-key/chord dispatch logic, only if it ends up custom (i.e., only if gpui's native chords don't cover the leader-key shape and the fallback state machine is built).
- **No tests for painting, layout, or hit-testing** — these are verified by dogfooding each milestone, same as Phase 9's practice (GPU windows can't be captured/verified from the agent sandbox).
- **Repaint-reliability** is a manual dogfood step at M0/M1, not automated harness (see above).
- **Keybind regression checklist:** before M5 merges, every entry in AGENTS.md's keybind table is manually exercised against the gpui build.
- **Standard gates apply before the M5 merge:** `cargo clippy`, `cargo fmt`, `scripts/ci-local.sh`.

## M0 Findings (2026-08-30, human dogfood on gpui 0.2.2, real macOS window)

**Repaint reliability: initially FAIL, then FIXED.** PTY output arriving async on a background reader
thread never triggered a gpui repaint on its own — `cx.notify()` was only ever called from `on_key_down`,
so output stayed invisible until the next unrelated keystroke. Reproduced exactly the
`gotcha_lost_pty_echo_wakeup`/Zed-vi-mode class of bug this check exists to catch, confirmed for real rather
than by inspection. Fixed with the spec-sanctioned M0 stand-in: a `cx.spawn` background loop polling
`cx.notify()` at ~30Hz. Verified fixed by the user directly (delayed output now appears with no manual
nudge). Real event-driven wakeup (PTY output directly notifying gpui, no polling) is real work, deferred to
M1 as the spec already anticipated.

**Ligature rendering: FAIL, confirmed real, not a config or approach mistake.** Default gives no ligatures
(gpui's default font/text_style; `TerminalGridElement` also shaped one character at a time, which makes
ligature substitution structurally impossible regardless of font — a shaper can only substitute e.g. `->`
for its combined glyph when both characters are in the same shaped run). Fixed the structural issue (shape
each full row as one `TextRun`) and switched to `MonoLisaCode Nerd Font` (the project's actual configured
ligature font) with explicit `calt`/`liga` `FontFeatures` set to 1. Nerd Font icon glyphs in the user's shell
prompt render correctly with this font (confirms the font itself resolves and basic glyph lookup works) —
but `-> == != >=` still renders as literal separate characters, reproduced identically across two separate
test rounds. Icon rendering success does not prove GSUB ligature substitution is wired up for an app-
constructed `Font`+`FontFeatures` value — those are different code paths (cmap lookup vs. shaping-time
substitution), and this result says the latter isn't working through gpui's public API at this version, for
this font, despite doing the structurally correct thing on our end. This matches the real, still-open Zed
issues the spec's original text-rendering section cited (`zed-industries/zed#11127`, `#48699`) — this looks
like gpui's actual current ceiling for this API surface, not something within our control to fix by trying
harder here.

**Fallback attempted: cosmic-text-shapes/paint_image-blits-atlas — ALSO FAIL, after real debugging, not
abandoned early.** Built per the spec's own named fallback: `cosmic_text::Buffer` shapes each grid row
(`Shaping::Advanced`), rasterized via `SwashCache`/`Buffer::draw` into an RGBA bitmap, painted into gpui via
`Window::paint_image` (verified against the actual pinned gpui 0.2.2 source, not `main`). Three real bugs
were found and fixed along the way (not given up on): (1) `FontSystem::new()` only does a bare system-font
scan and silently fell back to a non-ligature font — fixed by reusing this project's own proven
`font::loader::build_font_system` (resolves the font by file path via `FontLocator`, reports the font's
actual internal fontdb family name, which can differ from the config string); (2) the fix above needed the
right `FontConfig.family` value — the bare `Config::default()` used for M0 has a different code-level
fallback family (`"JetBrainsMono Nerd Font Mono"`) than what's actually configured for real use
(`MonoLisaCode Nerd Font`), which the font locator couldn't find at all (hard panic, no graceful fallback) —
fixed by setting the family explicitly; (3) removed a speculative explicit `cosmic_text::FontFeatures`/
`FeatureTag::enable(calt/liga)` call that this project's own proven, working code
(`font::shaper::make_attrs`) never uses — that function's own `_font_config: &FontConfig` parameter is
unused; ligatures in the existing wgpu renderer come from `Shaping::Advanced`'s defaults alone, not explicit
feature flags. After all three fixes — using the exact proven call shape, correct font, correct family
resolution — **ligatures still do not render**, reproduced identically.

Researched further per the user's explicit request before attempting a fourth fix: `pop-os/cosmic-term`
(cosmic-text's own official reference terminal application) has an open, unresolved issue
(`pop-os/cosmic-term#157`, reported March 2024, JetBrainsMono Nerd Font) reporting the same class of bug —
"some ligatures render, others don't." Its source (`src/terminal.rs`) was read directly: it builds one
`BufferLine` per row from `alacritty_terminal`'s `grid().display_iter()` with `Shaping::Advanced` —
architecturally the same row-based approach built here, not something idiosyncratic to this project's code.
At this point the working theory was "an ecosystem-wide, currently-unsolved terminal-grid ligature
limitation" — **this theory was WRONG, overturned by a direct test the user proposed and confirmed:**

**Root cause found: a locally broken/customized MonoLisa font file, not gpui or cosmic-text — and not even
MonoLisa's format generally.** Swapping only the font family (all other code unchanged — same
`build_font_system` resolution, same `Shaping::Advanced`) from `MonoLisaCode Nerd Font` to `PragmataPro Mono
Liga` (confirmed installed on the test machine) fixed ligature rendering — `->`, `!=`, `>=` all rendered as
correct, single combined glyphs, screenshot-verified. This isolated the problem to the MonoLisa font
specifically and confirmed the cosmic-text fallback pipeline (row-based `Shaping::Advanced`, `SwashCache::
draw`, `paint_image`) works correctly. Further investigation (MonoLisa's specimen page, `monolisa.dev/
specimen/code`) found the actual mechanism: MonoLisa gates its `->`/`==`/`!=`/`>=`-style combinations behind
OpenType Character Variant tags (`cv01`-`cv09`+ — "Arrows (cv08)", "Equal combinations (cv09)", matching the
exact glyphs tested), not `calt`/`liga`/stylistic-sets — explaining why the proven `calt`/`liga`-only call
shape (correct for every other font in this project) never worked for MonoLisa. Explicit `cv01`-`cv09`
`FontFeatures` were added to `terminal_element.rs` to request them directly.

**Then the user found the actual, final answer:** their installed MonoLisa font was a locally *customized*
build (via MonoLisa's own customizer tool, which — per its FAQ — can "freeze" a chosen feature set into the
font file, potentially stripping others) that had ligatures broken as an artifact of that customization —
not a stock MonoLisa limitation, not a gpui/cosmic-text defect, not even something this project's code could
have detected or fixed. Installing a fresh, non-customized MonoLisa font file fixed ligatures immediately.
The vendor FAQ's "weak feature support" framing was a real but ultimately secondary factor — the decisive one
was a broken local font file. (The `cv01`-`cv09` fix may or may not still be necessary against a fresh
MonoLisa install — not yet re-isolated — but is harmless to keep: a superset of feature requests, not a
behavior change for fonts that don't define those tags. The gpui-native path's original MonoLisa failure, and
the separate, real `cosmic-term#157`/Zed `#11127`/`#48699` issues, were never re-tested against a fresh font
install — this project's actual blocking question is answered regardless: ligatures work through the chosen
fallback, with a working font file.)

**Follow-on, separate finding (not a blocker):** `TerminalGridElement`'s `cell_width`/`cell_height` are
hardcoded constants (`px(9.0)`/`px(18.0)`) tuned loosely for the spike, not derived from the actual font's
real glyph metrics. With `PragmataPro Mono Liga` (narrower natural glyph width than what those constants
assume), the cursor quad renders visibly oversized relative to the text. Expected and appropriately deferred
— real font-metrics-driven cell sizing belongs to M1's "grid at parity" pass, not M0's proof-of-concept.

**Final result: PASS, with the project's real default font.** After installing a fresh, non-customized
MonoLisaCode Nerd Font, the user re-ran the exact same check (`echo '-> == != >='`) against the current code
(cosmic-text fallback, `build_font_system` font resolution, explicit `cv01`-`cv09` `FontFeatures`) and
confirmed: ligatures render correctly. M0's own question — "can this architecture render ligature-correct
terminal text under gpui at all, with the project's actual configured font" — is answered yes, via the
fallback. No product decision to defer; this works as shipped in the M0 spike's committed state.

**Leader-key chorded dispatch (Task 5): PASS.** `KeyBinding::new("ctrl-f %", SplitDemo, None)` — a bare,
non-modifier-chord leader key — worked via gpui's native chord matcher on the first attempt, no fallback to
a hand-rolled timeout state machine needed.

**Backspace: FAIL, fixed as a targeted addition (not originally in the spec's M0 scope).** Backspace has no
`key_char` (a control key, not printable text), so the spike's minimal char-forwarding `on_key_down` never
saw it — user flagged this as blocking basic dogfooding. Fixed via the same action/keybinding pattern
already proven for the leader-key split (`KeyBinding::new("backspace", Backspace, None)`, sends `0x7f` to
match what the existing wgpu app's real `key_map::translate_key` sends). Verified fixed by the user.

**Arrow keys / shell history / atuin: not working, confirmed expected and out of scope.** Same category as
backspace was (control keys with no `key_char`), not given a targeted fix since the spec's own scoping is
explicit: "Full key-event mapping (control chars, KKP, etc.) is out of scope for the spike." Real work for
whichever milestone builds out full keyboard parity, reusing PetruTerm's existing, already-correct
`src/app/input/mod.rs`/`key_map` translation logic rather than reimplementing it against gpui.

**Unplanned but required fix, not in the original spec text:** `term::Pty`/`term::Terminal::new`'s wakeup
mechanism was hard-coupled to `winit::event_loop::EventLoopProxy<()>`. Constructing ANY `winit::event_loop::
EventLoop` in the same process as gpui's own macOS Cocoa run loop crashes on startup — even one, even never
run (winit registers itself as the process-wide `NSApplication` delegate as a side effect of construction
alone, which conflicts fatally with gpui's separate Cocoa run loop). The spec had deferred this decoupling
to M2, assuming an unused throwaway `EventLoop` could sidestep it for M0 — that assumption was wrong. Fixed
by introducing `term::pty::Wakeup = Arc<dyn Fn() + Send + Sync>`, replacing the concrete winit type at the
`Pty`/`Terminal::new` boundary; the existing `petruterm` binary's `Mux` call sites wrap their real
`EventLoopProxy<()>` into a `Wakeup` at that boundary, unchanged above it. Reviewed and confirmed
behaviorally identical to the original for the existing binary (no regression risk).

**Known M0 shortcut, real follow-up needed (user flagged explicitly):** the font family
(`"MonoLisaCode Nerd Font"`) and the `cv01`-`cv09` OpenType feature list in `terminal_element.rs` are
hardcoded constants, not read from the user's real config. This spike never loads the user's Lua config
(`~/.config/petruterm/*.lua`) — it only uses `Config::default()` with the family field overridden inline.
Font family/features must become real settings (read from the user's actual config, the same way the
existing wgpu renderer's `FontConfig` already works) before this is anything more than a spike — this is not
something to leave hardcoded once the chrome migration moves past M0. Tracked here so it isn't lost.

## M1 — Grid at Parity (decomposed into ordered slices)

M1 as originally scoped (cursor, selection, scrollback, ligatures, emoji, LCD subpixel AA matching `master`,
plus the M0-flagged follow-ups: real settings, event-driven repaint, real cell metrics) is broad enough to
decompose rather than plan as one piece, per the user's explicit choice. Order, by dependency:

1. **M1a — Foundation fixes** (designed below, **COMPLETE**): real settings wiring, event-driven repaint, real
   font-metrics-driven cell sizing, plus full key-event mapping (an unplanned addition, discovered mid-work).
   Nothing else could be built *correctly* until these landed — cursor/selection positioning depends on real
   cell metrics, and the wrong font makes everything downstream moot.
2. **M1b — Grid parity** (designed below): ANSI colors, cursor shapes/blink, selection + copy + mouse-report
   passthrough, scrollback + visual scrollbar. The core "does this feel like the same terminal" milestone,
   built on M1a's correct metrics. Colors were folded in after M0/M1a work revealed the gpui grid renders
   everything in one fixed foreground/background — a bigger fidelity gap than anything the milestone
   originally named, and architecturally the same code path selection-highlight needs anyway.
3. **M1c — Visual polish**: emoji, LCD subpixel AA. Lowest risk, least foundational, last. Not yet designed.

### M1a — Foundation Fixes: Design

**Settings wiring.** `gpui_petruterm.rs`'s `main()` calls the real `config::load() -> Result<(Config,
mlua::Lua)>` (already used by the wgpu app — resolves `~/.config/petruterm/config.lua`, falls back to
embedded defaults) once at startup, instead of `Config::default()` with hardcoded overrides. `gpui_shell`'s
`spawn_terminal`/`TerminalGridElement` take the real `Config` instead of building their own. Hot-reload
included (user's explicit choice, not deferred): the existing `ConfigWatcher` (`config::watcher`, a
`notify`-based file watcher already built, delivering changed paths over an `mpsc::Receiver`) gets bridged
into gpui — a background thread loops on `ConfigWatcher::wait_timeout(...)`, and on a change calls
`config::reload()` and pushes the new `Config` across to `GpuiShellRoot` (which gets a `config: Config`
field that swaps in on reload). The cross-thread bridge mechanism is shared with repaint wake, below — "PTY
output arrived" and "config file changed" are the same shape of problem (an external thread producing an
event gpui's main thread needs to notice and act on).

**Event-driven repaint.** M0's ~30Hz unconditional poll (`cx.spawn` timer loop calling `cx.notify()`
regardless of whether anything changed) measured ~21-24% idle CPU — real, but the spec-sanctioned M0
stand-in, correctly scoped as "real work deferred" at the time. This is that real work, with an explicit
default and fallback (same pattern that served M0 well throughout):

- **Default:** research gpui's actual cross-thread wake API — not yet verified, this is the real open
  engineering question for M1a. Likely shape: gpui's background executor can run a blocking task (e.g.
  `cx.background_executor().spawn(async move { blocking_channel_recv() })`), and once that resolves inside
  gpui's own async world, the same `WeakEntity::update`/`cx.notify()` pattern already proven in M0 applies.
  Both the PTY `Wakeup` closure and the config-watcher's changed-path channel feed into this one bridge. If
  this holds, repaints happen immediately on real events, zero idle cost.
- **Fallback**, if that research comes up short: not a return to the unconditional poll, but a **smart
  poll** using `WakeupGate` (already exists in the app, `pending: AtomicBool` — referenced but never fully
  wired in M0, per its own Minor finding #4). The loop still runs on a timer, but only calls `cx.notify()`
  when `WakeupGate` actually has something pending — cutting idle CPU to near-zero without solving the
  harder cross-thread-async problem. A real, working fallback, not a placeholder.

**Cell metrics.** `font::shaper::TextShaper::measure_cell()` already computes real cell width/height (via
FreeType metrics, or a shaped-sample-string fallback) — but it's a private method on a `TextShaper` tied to
the wgpu-oriented glyph-atlas machinery, not something `gpui_shell` can call directly without dragging that
in. Since `gpui_shell` already builds its own `cosmic_text::FontSystem`/`Buffer` (via `build_font_system`,
from M0), the right-sized move is porting the *technique* (shape a sample string, read its real advance
width; use the font's line-height metric for cell height) into `gpui_shell`'s own font-init code, not
importing `TextShaper` wholesale. Computed once at startup and again on config hot-reload if the font
changes, stored alongside `FONT_SYSTEM`/`SWASH_CACHE`, replacing the `px(9.0)`/`px(18.0)` constants
everywhere they're currently used (grid sizing, cursor quad, row positioning) — this also closes the
glyph-position-drift gap M0's findings named as the real cost behind "the cursor renders oversized."

**Testing.** Same discipline as M0: dogfood for anything GPU/rendering/async-plumbing — not unit-tested, per
this migration's established approach. The one piece of real, pure business logic worth a unit test: the
smart-poll fallback's "only notify when `WakeupGate` has something pending" check, a plain boolean condition
independent of gpui/rendering. Standard gates apply: `scripts/ci-local.sh` (clippy, fmt, `cargo test --lib`,
audit) is the real project gate — not the narrower `cargo build`/`cargo test` substitute that let two real
findings slip through M0 until its final whole-branch review caught them.

### M1b — Grid Parity: Design

M1a's foundation work (real settings, real cell metrics, event-driven repaint, config hot-reload, full
key-event mapping) is complete and merged into this branch. M1b builds the actual "does this feel like the
same terminal" surface on top of it: ANSI colors, cursor shapes, selection, scrollback, and their input
handling. All four pieces share one architectural decision and reuse proven logic already in the codebase —
this is substantially a porting/wiring job, not new terminal-emulation logic.

**Mouse-handling architecture.** Each `TerminalGridElement` owns its own mouse handling — inside `paint()`
(which already has `bounds: Bounds<Pixels>` and the real cell metrics), register gpui's
`window.on_mouse_event::<MouseDownEvent/MouseMoveEvent/MouseUpEvent/ScrollWheelEvent>`, scoped to that
element's own bounds. Pixel-to-cell math reuses the same `bounds.origin`/cell-size arithmetic `paint()`
already does for cursor positioning. This mirrors the element's existing self-contained design (it already
owns cursor/text painting math scoped to its own bounds) and avoids the alternative — the parent `div`
owning all mouse handling — which would need extra plumbing to track each child element's bounds across
frames just for hit-testing, since gpui's flex layout doesn't expose child bounds until paint completes. The
one thing that must reach back to `GpuiShellRoot` is click-to-focus when there are split panes: a small
`on_focus` callback passed into each `TerminalGridElement` at construction time (in `render()`'s
`.children(...)` closure, which has `cx.listener` access), invoked on mouse-down.

**ANSI colors.** The gpui grid currently renders every cell in one fixed foreground
(`TEXT_COLOR = CosmicColor::rgb(0xf8, 0xf8, 0xf2)`) on one fixed background — `cell.fg`/`cell.bg` are never
read. Fix: extend `paint()`'s row-building loop (which currently only pushes `cell.c` into `grid_rows`) to
also collect `(AnsiColor, AnsiColor, CellStyle { bold, italic })` per cell, ported directly from
`src/app/mux/mod.rs`'s existing row-building logic — including its inverse-video swap
(`cell.flags.contains(Flags::INVERSE)` swaps fg/bg). Resolve colors via `term::color::resolve_color()`
(`src/term/color.rs`) — already a pure function taking `AnsiColor` + `ColorScheme`, no wgpu coupling, reused
as-is. Per-cell backgrounds are baked into the rasterized bitmap as filled rects before glyphs are drawn.
Each row is split into per-color/per-style spans before cosmic-text shaping (one `Attrs` per span instead of
one per row) — ligatures still shape correctly within a span; a color boundary landing mid-ligature is the
same accepted edge case the wgpu renderer already lives with. `CachedFrame`'s `content_hash` extends to
cover the color/style data (not just character text) so the frame cache still invalidates correctly when
only colors change (e.g. a re-colored but textually-identical prompt redraw).

**Selection highlight.** Reuses the exact same mechanism as colors, not a separate overlay: selected cells
get their fg/bg swapped before resolution, exactly like `src/app/mux/mod.rs`'s `cell_in_selection()` check
(ported as-is). `content_hash` also covers the current selection range.

**Cursor shapes + blink.** `Terminal::cursor_info()` already returns real shape (`Block`/`HollowBlock`/
`Underline`/`Beam`/`Hidden`) — port the pixel-geometry table from `src/app/renderer/terminal.rs`'s
`build_cursor_overlay` (Block/HollowBlock: full cell; Underline: bottom 2px; Beam: left 2px). `HollowBlock`
(vs `Block`) is shown when a pane isn't the split-focus target — reuses the same `is_active`-style signal
`TerminalGridElement` needs for click-to-focus, alongside the `on_focus` callback. Blink piggybacks on the
existing 33ms poll loop in `GpuiShellRoot::new`: a toggle every 530ms (matching
`update_cursor_blink`'s existing threshold in the wgpu app) calls `cx.notify()`, reset to visible-on by any
keystroke — no new timer infrastructure.

**Selection input, copy, mouse-report passthrough.** Click-drag selection ports `src/app/input/mod.rs`'s
`register_click` algorithm as-is (500ms same-cell window, click-count capped at 3, mapping 1→`SelectionType::
Simple`, 2→`Semantic`, 3→`Lines`). Mouse-down calls `terminal.start_selection`, mouse-move while the button
is held calls `terminal.update_selection`, mouse-up finalizes (both already exist on `Terminal`). Copy uses
gpui's own `cx.write_to_clipboard(ClipboardItem::new_string(terminal.selection_text()))` — gpui-native,
not the wgpu app's `arboard` dependency, per this migration's standing preference for gpui's own primitives
over hand-rolled/external equivalents where they cover the need. Before treating a click as local selection,
check `terminal.mouse_mode_flags()` (already exists) — if the terminal has mouse reporting enabled (vim,
tmux), send the escape sequence instead, porting `send_mouse_report`'s SGR (`\x1b[<{btn};{col};{row}M/m`) and
legacy X10 (`\x1b[M{btn+32}{col+32}{row+32}`, press-only) formats as-is.

**Scrollback + scrollbar.** Scroll wheel: gpui's `ScrollWheelEvent` delta converts to a line delta and calls
`terminal.scroll_display()` (already exists); unchanged behavior elsewhere (new PTY output or a keypress
still calls `scroll_to_bottom()`). Visual scrollbar: ports `src/app/renderer/overlay.rs`'s
`build_scroll_bar_instances` geometry as-is — a 6px-wide thumb on the right edge, sized
`(screen_rows / total_lines) * screen_rows` and positioned at `(1 - display_offset/history_size) * slack`
(display_offset=0 → thumb at bottom), drawn as a quad in `paint()` using `scrollback_info()` (already
exists). Drag-to-scroll on the thumb is new interaction handled by the same `TerminalGridElement` mouse
code: mouse-down inside the scrollbar's 6px strip starts a drag instead of a text selection; mouse-move
while dragging maps the new Y position back to a `display_offset` and calls `scroll_display`.

**File organization.** `terminal_element.rs` is already 457 lines (over this project's 400-line module
convention) before this milestone, and M1b adds substantially more — the split M1a's own whole-branch review
flagged as "plan before, not during" is due now. Split along the new responsibilities this milestone
introduces: font/metrics state (already fairly self-contained post-M1a) into its own file; the
color-resolution + cosmic-text rasterization logic (the single biggest addition) into its own file; the new
mouse-event handling (selection drag, click-count, mouse-report, scrollbar drag) into its own file — leaving
`terminal_element.rs` as orchestration, gluing metrics + rasterization + cursor + scrollbar painting
together via the `Element` impl. Exact file boundaries and task decomposition are a `writing-plans` decision.

**Testing.** Same discipline as M0/M1a: dogfood for anything GPU/rendering/mouse-pixel-math — not
unit-tested. Pure logic worth unit tests: click-count → `SelectionType` mapping, mouse-report
escape-sequence byte formatting (SGR/legacy X10), scrollbar thumb geometry (`thumb_rows`/`thumb_start` from
screen_rows/history_size/display_offset), and the selection/inverse-video fg/bg-swap logic.
`scripts/ci-local.sh` remains the real gate.

## M2 — Core Chrome: Design

M1 (a/b/c) is complete and merged into this branch: the terminal grid itself — colors, cursor, selection,
copy, mouse-report passthrough, scrollback, emoji — is at parity with `master`'s wgpu renderer for a single
terminal (plus a `SplitDemo` proof-of-concept second pane that has none of the real split-tree/tab
machinery). M2 builds the daily-use chrome around it: tabs, pane splits/resize/zoom, and the status bar —
the surfaces needed before this branch is usable as an actual terminal, not just a grid-rendering spike.
Ground truth for every piece below was read from the current wgpu implementation directly (file:line cited
throughout), not assumed.

**Scope, restated precisely against the parent spec's M2 line** ("Tabs, status bar, pane splits/resize/
zoom"): tab create/close/switch/rename + the tab bar UI, the pane-split tree (split/close/zoom/focus-by-
direction/ratio-resize + the draggable separator), the status bar (cwd/git-branch/exit-code/time segments),
and the leader-key chorded dispatch needed to drive all of the above from the keyboard (M1a's key mapping
only ever handled *un-chorded* keys — leader sequences are new). Sidebars, AI panel, command palette, context
menu, and search bar are explicitly M3/M4, not here.

### Architecture decision: port the pane-tree algorithms, replace the rect math with taffy flex

`PaneNode`/`PaneManager` (`src/ui/panes.rs`) is a ratio-based binary split tree with real, non-trivial domain
algorithms: `split`/`close_focused`/`remove_leaf` (tree mutation), `focus_dir` (nearest-center-in-the-
target-half-plane search), `adjust_parent_split` (walks to the nearest ancestor `Split` whose axis matches
the resize direction, preferring the deepest match), and `drag_split_ratio` (keyed by each `Split` node's
stable `node_id`, immune to concurrent relayout during a drag). These port as-is — the algorithms are the
value, not the `Rect`-based representation they currently compute into.

What does *not* port as-is is `PaneNode::layout`'s manual recursive `Rect` subdivision and `pane_infos`'s
manual pixel/cell-grid arithmetic (`src/ui/panes.rs:64-110`, `:429-522`) — gpui's `Div` is backed by a real
taffy flexbox engine (verified against gpui 0.2.2's own source: `.flex_row()`/`.flex_col()`, `.flex_basis
(relative(ratio))`, `.flex_1()` all exist as first-class styling methods), which computes exactly this kind
of nested-ratio layout natively. `PaneTree` (the ported version of `PaneNode`, renamed since it no longer
carries a `rect` field — taffy owns that now) is walked by `render()` into a nested `div()` tree: a `Split`
node becomes a `div().flex_row()` (or `.flex_col()` for `Vertical`) with two children, the earlier child
sized `.flex_basis(relative(node.ratio))` and the later `.flex_basis(relative(1.0 - node.ratio))`; a `Leaf`
node becomes a `TerminalGridElement` wrapped in a `div().flex_1()`. This eliminates `pane_infos`'s manual
pixel math and the 1-cell separator inset (`PanePad`) entirely — a real `div()` separator between panes
(below) supplies its own width, and cosmic-text rasterization already fills exactly its `TerminalGridElement`
parent's bounds, so no manual padding calculation is needed.

Terminal PTY sizing still needs real column/row counts, which taffy's *rendered* rect gives only after
layout, not before — `TerminalGridElement::request_layout` already computes its own size from
`cols × cell_width`, and the reverse direction (given a flex-computed pixel rect, resize the PTY to fit)
needs a `prepaint`-time (or post-frame) hook that reads back the element's `bounds: Bounds<Pixels>` and calls
`Terminal::resize(cols, rows, ...)` when it changed since the last frame — mirroring what `mux::resize_all`
(`src/app/mux/mod.rs:900-922`) already does today (recompute every leaf's pixel rect, resize any terminal
whose cell dimensions changed), just sourcing the rect from gpui's own layout pass instead of `pane_infos`.

### Tabs

`TabManager`/`Tab`/`tab_display_label` (`src/ui/tabs.rs`, all pure data + one pure string-formatting
function, zero I/O) port with no changes. `GpuiShellRoot` gains `tabs: TabManager` and changes `terminals:
Vec<Rc<Terminal>>` to a per-tab structure: each `Tab` now owns a `PaneTree` (the ported `PaneManager`) rather
than the flat `Vec` `SplitDemo` currently uses — `SplitDemo`'s ad hoc pane list is retired, replaced by real
`cmd_split`/`cmd_close_pane` (ported from `Mux`, see below) driving a real tab-indexed pane tree.

The tab bar itself: current wgpu visual is flat rects (an active-tab background fill + a bottom accent-color
underline) plus dimmed text for inactive tabs — *not* the "pill/SDF" shape prior project memory claimed
(verified directly against `src/app/renderer/overlay.rs:1091-1140`; that memory is stale and is corrected
here). No hover state exists today either. M2 reproduces exactly this — flat background + underline, active/
inactive only — as a `div()` row above the pane area, one child `div()` per tab calling `tab_display_label`
for its text, with `on_mouse_down` calling `TabManager::switch_to_index` directly (gpui's own hit-testing
replaces `hit_test_tab_bar`'s manual pixel math entirely — that function and its "renderer and hit-test share
one column-math function" TD-P9-02 workaround have no gpui equivalent because real elements don't need it).
Tab drag-reordering does not exist in the current app (confirmed: no such code anywhere in the codebase) and
is not built here either — this is parity work, not a new feature; note it as a candidate for a later
milestone if wanted, using gpui's `.on_drag()`/`.on_drag_move()` (confirmed present) rather than hand-rolled
pixel tracking.

### Panes: split, close, zoom, focus-by-direction, resize

`Mux`'s pane-mutation methods (`cmd_split`, `cmd_close_pane`, `cmd_toggle_zoom_pane`, `cmd_focus_pane_dir`,
`cmd_adjust_pane_ratio`, `cmd_drag_separator` — `src/app/mux/mod.rs:811-896`) are already engine-agnostic:
each is a thin wrapper calling into `PaneManager`/`PaneTree`, with no wgpu/winit coupling. They port as
`GpuiShellRoot` methods operating on the active tab's `PaneTree`, called from leader-key dispatch (below).
`cmd_split` spawns the new terminal *before* mutating the tree (existing TD-018 safety property, preserved)
using `spawn_terminal` (already exists in `gpui_shell/mod.rs`).

Zoom is *not* tree state in the wgpu app — it's a single `Option<usize>` on `Mux` (`zoomed_pane`), applied as
a render-time filter that swaps in one full-viewport `PaneInfo` instead of building the real tree's rect
list for that frame (`src/app/frame.rs:696-715`). Ported the same way: `GpuiShellRoot` gains `zoomed_pane:
Option<usize>`; `render()`'s children-building logic, when it's `Some(id)`, renders *only* that terminal's
`TerminalGridElement` at full size instead of walking the `PaneTree` into nested flex `div()`s — same
"render-time filter, no tree mutation" property, ported to gpui's declarative-tree-building idiom instead of
an imperative instance-list swap.

Separator drag: no native gpui resize-handle widget exists (confirmed against gpui 0.2.2 source) — a real
1-cell-wide `div()` is placed between each `Split` node's two children (i.e. exactly where the pane-tree ↔
flex-tree walk emits a `Split`), styled with `.cursor_col_resize()`/`.cursor_row_resize()` (both confirmed
present) for the hover affordance, and driven by the same `window.on_mouse_event`-based
down/move/up pattern this branch already uses for the scrollbar thumb and text selection (`gpui_shell/
mouse.rs`) — not gpui's drag-and-drop system (`.on_drag()` renders a *new floating preview view* following
the cursor, the wrong shape for "repaint two flex siblings' `flex_basis` as the pointer moves"). The drag
arithmetic itself ports as-is: `drag_split_ratio`'s node-id-keyed, mouse-position-relative-to-cached-rect
formula (`src/ui/panes.rs:664-687`) — "cached rect" here becomes the separator `div()`'s own last-painted
`bounds`, read the same way `TerminalGridElement`'s mouse handlers already read their own `bounds` in
`paint()`.

`Leader %`/`"` (split), `Leader x` (close pane), `Leader z` (zoom), `Leader h/j/k/l` (focus direction),
`Leader Option+arrows` (resize) all call the ported `cmd_*` methods above. The wgpu app's
sticky-resize-mode state machine (first `Leader Option+Arrow` press enters `resize_mode`, subsequent bare
arrow presses keep resizing without re-pressing leader, `src/app/input/mod.rs:219-310`) ports as a
`resize_mode: bool` field alongside the leader state (next section) — same shape, new home.

### Leader-key chorded dispatch

M1a's key handling (`gpui_shell/key_map.rs`) only ever translates a single, un-chorded keystroke into PTY
bytes — there is no leader-key concept yet. Every new keybind this milestone needs (`Leader c/&/n/b/,/%/"/x/
z/h/j/k/l`, `Leader Option+arrows`, plus bare `Cmd+1-9`) requires it. gpui does have a native chorded-keymap/
action-dispatch system (already used for the trivial single-chord `SplitDemo` binding), but a leader
sequence — prefix key, 1000ms timeout, then a context-free single follow-up key, with `Option+Arrow`
specifically needing to *stay* active across repeated presses without re-invoking the prefix — is a
stateful, timing-dependent shape gpui's declarative keymap is not built to express (confirmed: no
"chord with timeout and a fallback" primitive found in gpui 0.2.2's keymap source). Ported the same way the
existing wgpu-side hand-rolled state machine already solves it (`src/app/input/mod.rs`'s `leader_active`/
`leader_deadline`/`leader_prefix` fields, `:25-72`): `GpuiShellRoot` gains the same three fields (plus
`resize_mode` from above), `on_key_down` checks/sets `leader_active` on the configured leader key, and the
existing 33ms poll loop (already driving cursor blink) also checks `leader_deadline` each tick to expire a
stale leader press — no new timer infrastructure, same pattern M1b's blink used. The *mapping* from a
leader-key sequence to an action stays data-driven from Lua config exactly as today
(`config::keybind_view::leader_bindings_view`, already engine-agnostic, ported unchanged) — only the
*dispatch mechanism* (translating gpui's `KeyDownEvent` into that lookup) is new code. `Cmd+1-9` is a
plain (non-leader) binding, ported as a direct `event.keystroke.modifiers.platform && key is a digit` check
in `on_key_down`, matching the pattern already used for Cmd+V paste.

### Status bar

`StatusBar`/`StatusBarSegment`/`StatusBar::build` (`src/ui/status_bar.rs`, a pure function over already-
fetched inputs) port unchanged. Rendered as a `div()` row: each segment is its own `div()` with the
segment's fg/bg and text, and — since real elements get real hit-testing — `on_mouse_down` on the git-branch
segment and the exit-code segment directly, replacing `click_kind`'s manual column-math re-derivation
entirely (same simplification the tab bar gets). Segment content sources:

- **CWD**: `Mux::active_cwd` (`src/app/mux/mod.rs:302-305`, OS `proc_pidinfo`/`/proc/pid/cwd` lookup from the
  focused terminal's `child_pid`) is already engine-agnostic — called once per relevant state change (tab
  switch, focus change, matching `refresh_status_cache`'s existing call sites) rather than every frame.
- **Exit code**: sourced from the same mtime-gated per-PID shell-integration JSON file
  (`src/llm/shell_context.rs`) the wgpu app already reads — no logic changes, just move the poll call site
  into the existing 33ms tick loop.
- **Git branch**: the one genuinely new piece of async plumbing this milestone needs. The wgpu app's version
  (`src/app/ui/git.rs:6-60`) is real cross-thread machinery — a `tokio::spawn`'d fetch, a channel drained on
  each tick, a 15s TTL (60s in battery-saver), and a 30s stuck-in-flight recovery timeout — tightly coupled
  to winit's `about_to_wait` loop. Ports using the bridge pattern this branch already established for config
  hot-reload (`gpui_shell/mod.rs`'s `PENDING_CONFIG_RELOAD`/`CONFIG_CHANGED` statics, `:82-128`): a
  `tokio::spawn`'d fetch writes its result into a similar static slot, and the existing 33ms poll loop reads
  it — same TTL/stuck-recovery *policy*, re-plumbed through gpui's `cx.spawn`/`cx.background_executor()`
  idiom instead of winit's event loop. Branch checkout (`git_checkout`) and the palette branch-*list* fetch
  are out of scope here — no command palette exists yet (M4).
- **Time**: direct `libc` call (`format_time`), no caching concern.

The whole status bar is drawn once per window (reflecting the *focused* terminal regardless of which pane),
not per-pane or per-tab — matches the current app exactly.

### File organization

New files under `src/gpui_shell/`: `panes.rs` (the ported `PaneTree`/split-tree algorithms — separate from
`mouse.rs`, which stays scoped to the terminal-grid's own click/drag/scroll handling, not pane-separator
drag), `tabs.rs` (ported `TabManager`/`Tab`, plus the tab-bar `Render` view), `status_bar.rs` (ported
`StatusBar`/`StatusBarSegment`, plus the status-bar `Render` view and the git-branch async bridge), `leader.
rs` (the chorded-dispatch state machine). `mod.rs` grows to own `GpuiShellRoot`'s new fields (`tabs`,
`zoomed_pane`, leader/resize-mode state) and the top-level `render()` restructuring (tab bar row above, pane
tree below, status bar row beneath) — expect it to need the same kind of split M1b gave
`terminal_element.rs` if it grows past the 400-line convention; exact boundaries are a `writing-plans`
decision once real line counts are known.

### Testing

Same discipline as M0/M1: dogfood for anything GPU/rendering/layout/mouse-pixel-math. Pure logic worth unit
tests: `PaneTree`'s tree-mutation algorithms (split/close/focus_dir/adjust_ratio — these are exactly the kind
of business logic the parent spec's "Testing & verification" section already calls out as in-scope,
independent of this being a chrome milestone), `drag_split_ratio`'s ratio-from-position math,
`tab_display_label`'s truncation/rename-cursor formatting, `StatusBar::build`'s segment assembly, the
git-branch fetch's TTL/stuck-recovery decision logic (given fake clock inputs, not real threads/time).
`scripts/ci-local.sh` remains the real gate.

## Explicitly out of scope / not being built

- No custom hit-testing framework — gpui's layout tree replaces it.
- No custom animation/easing system — gpui's built-in transition primitives are used as-is unless a real gap is found.
- No headless PTY test harness for repaint verification — manual dogfood only.
- No change to `alacritty_terminal` grid/PTY model, Lua config DSL, or LLM engine.
