# Technical Debt Registry

**Last Updated:** 2026-04-10
**Open Items:** 1
**Critical (P0):** 0 | **P1:** 1 | **P2:** 0 | **P3:** 0

> Resolved items are in [TECHNICAL_DEBT_archive.md](./TECHNICAL_DEBT_archive.md).

## Priority Definitions

| Priority | Definition | SLA |
|----------|------------|-----|
| P0 | Blocking development or causing incidents | Immediate |
| P1 | Significant impact on velocity or correctness | This sprint |
| P2 | Moderate impact, workaround exists | This quarter |
| P3 | Minor, address when convenient | Backlog |

---

## P1 — Alta prioridad

### TD-PERF-03: Upload completo del instance buffer a GPU cada frame
- **Archivo:** `src/app/mod.rs` → `src/renderer/gpu.rs`
- **Nota:** En Apple Silicon (M2/M4) con arquitectura unified memory, `write_buffer` es un memcpy en memoria compartida CPU-GPU. ~800KB a 60fps = ~48MB/s frente a 100+GB/s de ancho de banda — 0.05% del bandwidth disponible. **No es un cuello de botella real en Apple Silicon.** Sería relevante en GPUs discretas con PCIe.
- **Fix futuro:** Dirty-rect tracking por fila para reducir el volumen de upload. Dejar para cuando haya soporte cross-platform (Phase 2+).
