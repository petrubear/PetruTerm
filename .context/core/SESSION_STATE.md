# Session State

**Last Updated:** 2026-09-17
**Session Focus:** gpui chrome migration — milestone completion (M0-M5d) plus a visual-polish
dogfood pass on `gpui_shell`'s chrome, matching an approved design mockup.

## Branch

`worktree-gpui-migration` (permanent worktree at `.claude/worktrees/gpui-migration`, forked
from `master` at `03b0071`). **Never merges to master** — this is a standing, indefinite
constraint, not a milestone-completion decision to revisit each session. `master` is untouched
by this branch's work and continues to receive its own unrelated commits independently.

## Project Status

- The wgpu binary (`petruterm`, on `master`) is unaffected and still the shipped product.
- The gpui binary (`gpui-petruterm`, this branch only) is feature-complete through M5d: full
  grid parity, core chrome (tabs/panes/status bar), sidebars (workspaces/MCP/skills/steering),
  AI chat panel + inline block, command palette, search, context menu, toasts, ACP agent
  backend (now the default for both binaries), and prompt-context injection (skills/steering/
  MCP) at parity with the wgpu build.
- TD-GPUI-01..06 + TD-GPUI-ACP debt cleanup: all RESOLVED (`.context/quality/TECHNICAL_DEBT.md`,
  "Migración gpui — COMPLETA" section).
- This session's own work (all on top of an already-complete M5d): bundled PetruTheme Dark/Light
  themes, a fix for a pre-existing bug where `config.colors` was silently never applied, a
  standalone `.app` bundle + custom icon for the gpui binary (coexists with the wgpu `.app`),
  and a multi-round visual-polish pass rebuilding `gpui_shell`'s chrome as floating cards
  (margin/gap/radius/border on every region) to match an approved mockup — sidebar drag-resize,
  corner-clash and content-alignment fixes across all three header rows, a shared
  `header_row_min_height()` so headers stay the same height regardless of each row's own font
  size, per-tab custom-color display fixed to show on inactive tabs too, and the battery
  status-bar widget ported from the wgpu build.
- Dogfooded live throughout via `/Applications/PetruTerm-gpui.app` (distinct bundle ID/icon
  from the wgpu `.app`) — every polish-pass commit was rebuilt and reinstalled for the user to
  re-check against a live screenshot before the next fix.

## Completed This Session

1. `d398a63` — Bundled PetruTheme Dark/Light themes (`assets/themes/petru-theme-{dark,light}.lua`).
2. `d87c942` — Fixed `config.colors` never being read from `config.lua`/`ui.lua` (pre-existing
   bug, not introduced this session); added a regression test.
3. `720d863`/`094c65e` — `scripts/bundle-gpui.sh` + a hue-shifted custom icon so the gpui `.app`
   is visually distinct from the wgpu `.app`.
4. `6f92648`..`4087a7d` (11 commits) — the visual-polish dogfood arc: floating-card chrome
   rewrite, sidebar drag-to-resize, three rounds of live-screenshot-driven fixes (double-padding,
   corner-clash vs. rounded corners, content-edge alignment across all three header rows), a
   shared header-height constant, the tab accent-color display fix, the battery widget port, and
   the chat header's icon/label getting the same pill treatment as the tab bar's active cell.

## Next Session Priorities

No open feature work queued. If continuing the gpui track, candidates raised but not yet
requested by the user: chat panel drag-to-resize (same `resize_handle.rs` element, unused so
far by anything but the sidebar), the wgpu binary's InputShadow/ghost-text/syntax-highlight
input decoration (confirmed gap in gpui_shell, explicitly deprioritized by the user — "the
shell covers that for me"), a handful of smaller ACP/UI parity gaps surfaced during a feature-
parity audit (Leader a c, skill/steering reload-on-switch, continuous hover-highlight, exit-code
popup, floating file picker) — reported, never requested or declined. Do not start any of these
without the user's explicit direction; this branch is in an active live-dogfood cycle where the
user drives the next fix from what they see running.
