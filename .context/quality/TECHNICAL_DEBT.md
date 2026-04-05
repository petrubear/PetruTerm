# Technical Debt Registry

**Last Updated:** 2026-04-04
**Open Items:** 2
**Critical (P0):** 0 | **P1:** 0 | **P2:** 2 | **P3:** 0

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

### TD-013 — Tab bar: rounded-rectangle pill tabs
- **File:** `src/app/renderer.rs` → `build_tab_bar_instances`
- **Problem:** Cell-based renderer can only draw rectangular segments. E0B4/E0B6 powerline glyphs are solid triangles, not rounded caps — they produce a zigzag arrow pattern instead of pills.
- **Fix:** Add a dedicated wgpu render pass (before the cell pass) that draws arbitrary rounded rectangles via a simple WGSL shader. Tab bar tabs would be submitted as rounded-rect primitives (position, size, radius, color) rendered on the GPU, then cell glyphs (text) composited on top.
- **Effort:** Medium — new pipeline + vertex buffer, but isolated from the existing cell pipeline.

### TD-014 — Tab bar: background color mismatch
- **File:** `src/app/renderer.rs` → `build_tab_bar_instances`, `src/config/schema.rs`
- **Problem:** `BAR_BG` is hardcoded to `[0.10, 0.10, 0.15]`. Should match `config.colors.background` so the tab bar blends with the terminal background, or be fully transparent (relying on the window background).
- **Fix:** Pass `config.colors.background` into `build_tab_bar_instances` and use it as `BAR_BG`. Longer term: make the tab bar row transparent so the window/terminal background shows through.
- **Effort:** Small — single color substitution once TD-013 GPU pass exists (transparency is free there).

---

## P3 - Low Priority

_None_

---

## Open Debt Summary

| ID | Priority | Description |
|----|----------|-------------|
| TD-013 | P2 | Tab bar rounded-rectangle pills (requires GPU render pass) |
| TD-014 | P2 | Tab bar background should match terminal bg / be transparent |
