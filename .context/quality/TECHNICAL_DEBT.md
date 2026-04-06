# Technical Debt Registry

**Last Updated:** 2026-04-06
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

- _None_

---

## P1 - High Priority

- _None_

---

## P2 - Medium Priority

- _None_

---

## P3 - Low Priority

- _None_

---

## Recently Resolved (2026-04-06)

- **TD-OP-02** (P1): `is_pua()` had 5 redundant subranges (Devicons 0xE700, Font Awesome 0xF000, Seti-UI 0xE5FA, Font Logotypes 0xE200, Weather 0xE300) that were all subsets of the BMP PUA block `0xE000..=0xF8FF`. Caused `unreachable_patterns` warnings. Removed all redundant arms; doc-comment now explains what BMP PUA covers.
- **TD-OP-03** (P2): `GlyphAtlas` had a 2048×2048 texture and no eviction policy. Upgraded to 4096×4096 (4× capacity). Added `epoch`-based LRU: each `AtlasEntry` carries `last_used: u64`; `next_epoch()` called once per frame; `evict_cold(max_age)` purges entries not touched in the last N epochs; `is_near_full()` triggers proactive eviction at 90% capacity before resorting to full `clear()`.
- **TD-OP-01** (P2): `unsafe impl Sync for TextShaper` was incorrect — FreeType's `FT_Library` is not thread-safe, allowing concurrent `&TextShaper` from multiple threads would be UB. Removed `Sync`. Kept `Send` with an explicit `// SAFETY:` comment documenting the single-owner, main-thread invariant.
- **TD-016** (P3): `last_assistant_command()` returned the raw `streaming_buf` content including tool-status lines (`⟳ list_dir(.) ✓ list_dir(.)`) prepended to the actual command. Fixed by filtering lines that start with `⟳` or `✓` before applying the markdown-fence strip.

> **TD-015** (resolved 2026-04-05): Shift+Enter → `\x1b[13;2u`, Shift+Tab → `\x1b[Z`.
> **TD-013** (resolved 2026-04-05): Rounded tab pills via `RoundedRectPipeline` + SDF WGSL shader.
> **TD-014** (resolved 2026-04-05): Tab bar background inherits `config.colors.background`.
