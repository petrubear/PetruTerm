# Technical Debt Registry

**Last Updated:** 2026-04-10
**Open Items:** 4
**Critical (P0):** 0 | **P1:** 1 | **P2:** 2 | **P3:** 1

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

---

## P2 — Prioridad media

### TD-PERF-04: `scan_files()` sincrónico en el hilo principal al abrir el file picker
- **Archivo:** `src/llm/chat_panel.rs` → `open_file_picker()` / `scan_files()`
- **Descripción:** Al abrir el file picker (`Tab`), se llama `scan_files(cwd, depth=3)` de forma sincrónica en el hilo principal del event loop. En un repositorio grande (ej. un monorepo con miles de archivos en 3 niveles), esto puede bloquear el render loop durante decenas de milisegundos, causando un frame drop visible.
- **Fix:** Mover `scan_files` a un `tokio::task::spawn_blocking` y enviar el resultado al hilo principal via canal. Mostrar un spinner mientras carga.
- **Severidad:** P2 — no afecta la funcionalidad pero degrada la UX en repos grandes.

---

### TD-PERF-05: Atlas de glifos siempre 64 MB de VRAM desde el arranque
- **Archivo:** `src/renderer/atlas.rs` → `GlyphAtlas::new()`
- **Descripción:** El atlas se crea como una textura RGBA de 4096×4096 px = 64 MB de VRAM en el momento del arranque, independientemente de cuántos glifos se usen realmente. Para una sesión con una sola fuente y ASCII básico, la mayoría de esa memoria nunca se toca. En sistemas con VRAM limitada (iGPU con memoria compartida configurada al mínimo), esto puede presionar el presupuesto de memoria.
- **Fix:** Empezar con un atlas más pequeño (ej. 1024×1024 = 4 MB) y crecer dinámicamente hasta 4096×4096 cuando se alcance el umbral de evicción. Requiere recrear la textura y re-subir los glifos calientes.
- **Nota:** En Apple Silicon con unified memory, el impacto es menor porque la VRAM es memoria del sistema. Relevante para Phase 2+ con GPUs discretas.
- **Severidad:** P2 — impacto bajo en el target actual (macOS/Apple Silicon).

---

## P3 — Baja prioridad

### TD-MAINT-01: `cargo-audit` no instalado — sin escaneo de CVEs en dependencias
- **Descripción:** El proyecto tiene ~40 dependencias directas y cientos transitivas. Sin `cargo-audit` en el flujo de CI/CD, no hay detección automática de vulnerabilidades conocidas (RustSec advisory database). Dependencias como `reqwest`, `tokio`, `mlua` y `wgpu` tienen historial de CVEs.
- **Fix:** Añadir `cargo install cargo-audit` al setup del entorno de desarrollo. Agregar `cargo audit` como paso en CI. Considerar `cargo-deny` para políticas de licencias y advisories.
- **Severidad:** P3 — no hay CVEs conocidos activos, pero es una brecha de proceso.

---

