# Technical Debt Registry

**Last Updated:** 2026-04-05
**Open Items:** 3
**Critical (P0):** 0 | **P1:** 1 | **P2:** 2 | **P3:** 0

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

- _None_

---

## P1 - High Priority

- **TD-OP-02**: Fragile Nerd Font rendering logic using manual glyph ID overrides. Heavily dependent on FreeType/cosmic-text mismatch workarounds. (origin: opencode — confirmed real)

---

## P2 - Medium Priority

- **TD-OP-03**: `GlyphAtlas` fragmentation and lack of eviction policy. Simple shelf packing leads to `AtlasError::Full` even when space is available via gaps. (origin: opencode — confirmed real)
- **TD-OP-01**: ~~opencode flagged as P0~~ — false positive. `FreeTypeCmapLookup::drop()` correctly calls `FT_Done_Face` then `FT_Done_FreeType` with null guards; no use-after-free. Real latent risk: `unsafe impl Send for TextShaper` while FT_Library internals aren't thread-safe, but `TextShaper` stays on the main thread in practice.

---

## P3 - Low Priority

- _None_

---

## Open Debt Summary

- **TD-OP-02**: Nerd Font Rendering Robustness (P1)
- **TD-OP-03**: Glyph Atlas Eviction/Packing Strategy (P2)
- **TD-OP-01**: FreeTypeCmapLookup unsafe Send (P2, was P0 false positive)

> **TD-015** (resolved 2026-04-05): `Shift+Enter` now sends `\x1b[13;2u` to PTY (xterm modified key sequence) instead of `\r`. Chat panel handles `Shift+Enter` as `\n` insertion. `Shift+Tab` also fixed to send `\x1b[Z` (reverse-tab). Files: `src/app/input/key_map.rs`, `src/app/input/mod.rs`.
>
> **TD-013** (resolved 2026-04-05): `RoundedRectPipeline` + SDF WGSL shader added in `src/renderer/rounded_rect.rs`. Tab pills now rendered as GPU rounded rects before the cell pass; `fs_bg` discals transparent-bg cells so text composites cleanly on top.
>
> **TD-014** (resolved 2026-04-05): Tab bar background now inherits the window clear color (= `config.colors.background`), removing the hardcoded dark constant.
