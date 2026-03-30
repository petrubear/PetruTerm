# Active Context

**Current Focus:** Phase 3 — Rendering Quality
**Last Active:** 2026-03-30
**Target Completion:** TD-027 (powerline vivid rendering)
**Priority:** P3

## Current State

**Phase 1 & 2 complete as of 2026-03-27.**
All acceptance criteria verified on M4 Max.

### Phase 1 Verified ✓
- Dracula Pro background `#22212c` ✓
- JetBrains Mono Nerd Font Mono 15pt, 18×36px at 2× Retina ✓
- zsh + Starship, keyboard input (including Ctrl keys), `ls` output ✓
- Mouse: drag selection, scroll wheel (trackpad+mouse), SGR/X10 reporting ✓
- Clipboard: Cmd+C/V, OSC 52, bracketed paste ✓
- Cursor: block/underline/beam, 530ms blink, resets on keypress ✓
- PTY resize: uses actual cell px from TextShaper ✓
- Shell exit: `exit` / Ctrl+D closes window ✓
- Nerd Font icons: clamped to cell height, no row bleeding ✓
- Config hot-reload ✓
- Custom title bar: transparent, traffic lights, draggable ✓
- Launch directory: opens in `~` ✓
- .app bundle: `dist/PetruTerm.app` (18 MB, ad-hoc signed) ✓
- App icon: Dracula purple chevron + cursor ✓
- Scrollback: 110k lines, display_offset-aware rendering ✓
- Top padding: 60px physical clears traffic lights ✓
- Arrow keys APP_CURSOR mode (atuin, nvim, tmux) ✓
- Reverse-video (SGR 7 / Flags::INVERSE) ✓
- nvim: colors, cursor, input, scroll ✓
- tmux: attach, split, scroll, Ctrl+B prefix ✓
- Font ligatures: `->` `=>` `==` `===` `!=` `>=` `|>` ✓

## Next Session Scope — Rendering Quality

### Priority Order
1. ~~**TD-025** — Line spacing~~ **DONE** — `font.line_height: f32` (default 1.2), propagated via Metrics → cell_height → PTY.
2. ~~**TD-026** — Antialiasing quality~~ **DONE (all 3 levels)** — TD-026a gamma correction, TD-026b background-aware blending, TD-026c LCD subpixel AA.
3. **TD-027** — Powerline separator vivid rendering — OPEN. Hybrid bg-aware premul in `fs_main` is best current approach. See TD-027 in TECHNICAL_DEBT.md for next steps.

### Out of Scope (Phase 3)
- `src/plugins/` — Phase 3
- `src/snippets/` — Phase 3
- `src/ui/statusbar/` — Phase 3
- TD-022 Agent mode — needs design doc
- TD-023 Leader key — polish

## Files to Reference
- `.context/specs/build_phases.md` — Phase 2 deliverables checklist
- `.context/specs/term_specs.md` — authoritative spec
- `.context/quality/TECHNICAL_DEBT.md` — open debt items
- `.context/core/SESSION_STATE.md` — session notes + Phase 2 order
