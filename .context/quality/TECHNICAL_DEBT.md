# Technical Debt Registry

**Last Updated:** 2026-04-05
**Open Items:** 0
**Critical (P0):** 0 | **P1:** 0 | **P2:** 0 | **P3:** 0

> Resolved items are in [TECHNICAL_DEBT_archive.md](./TECHNICAL_DEBT_archive.md).

## Priority Definitions

| Priority | Definition | SLA |
|----------|------------|-----|
| P0 | Blocking development or causing incidents | Immediate |
| P1 | Significant impact on velocity or correctness | This sprint |
| P2 | Moderate impact, workaround exists | This quarter |
| P3 | Minor, address when convenient | Backlog |

---

## P0 - Critical

_None_

---

## P1 - High Priority

_None_

---

## P2 - Medium Priority

_None_

---

## P3 - Low Priority

_None_

---

## Open Debt Summary

_Clean — no open items as of 2026-04-05._

> **TD-015** (resolved 2026-04-05): `Shift+Enter` now sends `\x1b[13;2u` to PTY (xterm modified key sequence) instead of `\r`. Chat panel handles `Shift+Enter` as `\n` insertion. `Shift+Tab` also fixed to send `\x1b[Z` (reverse-tab). Files: `src/app/input/key_map.rs`, `src/app/input/mod.rs`.
>
> **TD-013** (resolved 2026-04-05): `RoundedRectPipeline` + SDF WGSL shader added in `src/renderer/rounded_rect.rs`. Tab pills now rendered as GPU rounded rects before the cell pass; `fs_bg` discards transparent-bg cells so text composites cleanly on top.
>
> **TD-014** (resolved 2026-04-05): Tab bar background now inherits the window clear color (= `config.colors.background`), removing the hardcoded dark constant.
