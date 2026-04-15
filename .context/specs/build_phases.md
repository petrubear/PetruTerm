# PetruTerm — Build Phases

> Phases 0.5–3 (complete) archived in [`build_phases_archive.md`](./build_phases_archive.md).

---

## Phase 3.5: Performance Sprint ⚡
**Status: Not started — P1 crítico, prerequisito para Phase 4**

**Goal:** Hacer de PetruTerm el terminal más rápido en existencia.

| KPI | Target |
|-----|--------|
| Input-to-pixel latency p99 | < 8 ms |
| Input-to-pixel latency p50 | < 4 ms |
| Steady-state frame time | < 1 ms |
| Idle allocations | 0 |
| Cache-miss storm | < 16 ms |
| Startup (exec → first pixel) | < 80 ms |

**Principio rector:** *Measure first, optimize second.* Sub-phases B-H no empiezan hasta que A esté completa.

---

### Sub-phase A: Measurement Infrastructure (prerequisite)

**Baselines (2026-04-14, M4 Max, release profile):**
| Benchmark | Tiempo |
|-----------|--------|
| `shape_line_ascii` | 5 643 ns |
| `shape_line_ligatures` | 8 766 ns |
| `shape_line_unicode` | 5 586 ns |

- [x] `benches/shaping.rs` con criterion: `shape_line_ascii`, `shape_line_ligatures`, `shape_line_unicode`
- [x] Tracing con `tracing` + feature flag `profiling`: spans en `build_instances`, `shape_line`, `RedrawRequested`
- [x] Debug HUD (F12): overlay con last frame time, p50/p95 ring buffer (120 frames), shape cache hit/miss %, atlas fill %, instance count
- [x] `.context/quality/PROFILING.md`: recetas para criterion, flamegraph, Instruments
- [ ] Frame budget documentado en `term_specs.md`: objetivos p50/p95/p99
- [ ] Benches adicionales: `build_instances` (1/4 panes), `search_active_terminal` (1K/10K/100K history), `rasterize_to_atlas` cold miss
- [ ] CI gating: regresión > 5% en bench principal falla el build
- [ ] Tracy integration (opt-in): `tracing-tracy` subscriber
- [ ] GPU timestamp queries: `wgpu::QuerySet::Timestamp`
- [ ] Input-to-pixel latency probe: ring buffer `KeyboardInput` → frame end
- [ ] `os_signpost` markers en macOS

**Exit criteria:** frame time visible en vivo; criterion reproduce mediciones; latency probe reporta números coherentes.

---

### Sub-phase B: Idle Zero-Cost (steady state)

- [ ] Cursor como overlay independiente (elimina TD-PERF-10): blink solo cambia alpha, no invalida grid cache
- [ ] Damage tracking con `alacritty_terminal::Term::damage()`: solo iterar filas en damage set
- [ ] Idle detection + adaptive frame rate: `ControlFlow::Wait` cuando sin PTY data, input, ni AI event
- [ ] No-op render path: si solo cambió cursor blink y grid idle, saltar `build_all_pane_instances` completamente
- [ ] Counter de bytes uploaded al GPU por frame: idle debe ser 0 bytes
- [ ] Mover `poll_git_branch` a timer 1 Hz independiente del render loop (fixea TD-PERF-19)

**Exit criteria:** tracy muestra idle frames < 100 µs. HUD reporta 0 bytes uploaded en idle.

---

### Sub-phase C: Hot Path Fast Paths (typing & scrolling)

- [ ] ASCII fast path en `shape_line`: saltar HarfBuzz para ASCII-only sin chars de ligatura
- [ ] Pre-shape warmup al arranque: rasterizar ASCII 32-126 + common Nerd Font icons; marcar `hot=true` (never evict)
- [ ] Per-word shape cache: `(word, font_key) → Vec<ShapedGlyph>` con xxhash3
- [ ] Ligature scan bit: `memchr` scan antes de HarfBuzz — si no hay chars de ligadura, saltar HB
- [ ] Subpixel position quantization: cuantizar offsets a 1/4 px antes de cachear
- [ ] Empty/space cell fast path: celdas ` ` + bg default saltan el glyph pipeline
- [ ] Row hash con xxhash3 o foldhash (reemplaza DefaultHasher)

**Exit criteria:** criterion muestra `shape_line("fn hello_world()")` < 5 µs. ASCII fast path hit rate > 90%.

---

### Sub-phase D: Memory & Allocator

- [ ] `mimalloc` como `#[global_allocator]`: evaluar con microbenchmarks; rollback si no hay win
- [ ] Bumpalo arena para datos per-frame en `RenderContext`: reset O(1), cero malloc en hot path
- [ ] Scratch buffers a `RenderContext`: `scratch_chars`, `scratch_padded`, `scratch_colors`, `scratch_lines` (reemplaza TD-PERF-12, TD-PERF-13)
- [ ] `smallvec::SmallVec<[T; N]>` en hot paths (shaped glyphs por run, search matches por fila)
- [ ] `compact_str::CompactString` donde contenido es corto (tab titles, paths relativos)
- [ ] Color palette interning: `ColorId(u16)` → lookup table como uniform buffer (solo si HUD muestra bottleneck)

**Exit criteria:** `dhat-rs` report muestra 0 allocs en hot loop tras warm-up.

---

### Sub-phase E: Parallel Rendering

- [ ] Rayon per-pane parallel build: `par_iter` sobre `pane_infos`, merge en main thread
- [ ] Parallel row shaping en cache-miss storm: rayon para filas con miss > threshold
- [ ] Atlas upload en secondary queue: Metal multi-queue para no bloquear render pass
- [ ] Lock-free PTY ring buffer: reemplazar crossbeam-channel con `rtrb` SPSC
- [ ] PTY reader thread pinned a efficiency core: `pthread_set_qos_class_self_np(QOS_CLASS_UTILITY)`

**Exit criteria:** 4-pane build en tiempo cercano al pane más lento. Multi-pane benchmarks mejoran 2-3× en M4.

---

### Sub-phase F: Latency Minimization

- [ ] `PresentMode::Mailbox` con fallback a FifoRelaxed → Fifo; exponer como `config.performance.present_mode` (reemplaza TD-PERF-08)
- [ ] `desired_maximum_frame_latency = 1`: de 2 a 1
- [ ] Adaptive PTY coalescing: saltar 4 ms coalesce para single-char typing; mantener para TUI bursts (> 3 eventos/ms)
- [ ] Input event priority sobre PTY en tick: procesar `KeyboardInput`/`MouseInput` antes que PTY drain
- [ ] `CVDisplayLink` en macOS (experimental): firing exactamente en vblank start
- [ ] `CAMetalLayer::setDisplaySyncEnabled(false)` para low-latency mode (experimental)
- [ ] Skip render cuando window hidden/minimized: detectar `WindowEvent::Occluded(true)`
- [ ] Input latency test suite: script que simula typing a 100 cps + latency probe

**Exit criteria:** latency probe p50 < 4 ms, p99 < 8 ms en typing en zsh idle.

---

### Sub-phase G: GPU Architecture (forward-looking)

- [ ] Atlas split por tamaño: 1024×1024 para ASCII + 4096×4096 para emoji/wide
- [ ] Persistent mapped ring buffer (3× frame in flight)
- [ ] Indirect draw para multi-pane: un solo `draw_indirect` con `DrawIndirectArgs` per pane
- [ ] Unificar bg + glyph en un solo pass
- [ ] GPU-resident grid (experimental, tipo Kitty): documentar como "Phase 5+ candidate"
- [ ] Compute shader shaping (experimental): documentar como exploratory

**Exit criteria:** primeros 4 items completados. Últimos 2 documentados como spike tasks.

---

### Sub-phase H: Build & Release Optimization

- [ ] PGO: `cargo pgo` con workload representativo (typing zsh, nvim, scroll de log)
- [ ] BOLT post-link: `llvm-bolt` con profiles generados
- [ ] `target-cpu=apple-m1` en `bundle.sh` para el `.app` bundle
- [ ] `release-native` profile en `.cargo/config.toml` con `target-cpu=native` + `lto=thin`
- [ ] Lua bytecode cache: precompilar `.lua` a bytecode mlua, cachear en `~/.cache/petruterm/lua-bc/`
- [ ] Config eager-load en paralelo con window creation
- [ ] Split debug info: `split-debuginfo = "unpacked"` en release

**Exit criteria:** startup < 80 ms. Runtime benchmarks mejoran 5-10% vs release sin PGO/BOLT.

---

### Exit Criteria (Phase 3.5 global)

- [ ] Input-to-pixel latency: **p50 < 4 ms, p99 < 8 ms**
- [ ] Steady-state frame time: **< 1 ms**
- [ ] Zero allocations en idle hot path (verificado con `dhat`)
- [ ] Cache-miss storm: **< 16 ms**
- [ ] Startup: **< 80 ms** wall-clock
- [ ] Criterion benchmarks con CI gating activo
- [ ] Debug HUD (F12) operativo
- [ ] `PROFILING.md` con recetas reproducibles
- [ ] Comparativa vs Alacritty/kitty/wezterm/Terminal.app en `.context/quality/BENCHMARKS.md`

---

## Phase 4: Plugin Ecosystem
**Status: Not started — bloqueado hasta Phase 3.5 exit criteria**

**Goal:** Extensible plugin platform — third-party Lua plugins can extend palette, status bar, and events.

### Deliverables
- [ ] Plugin loader: auto-scan `~/.config/petruterm/plugins/*.lua`
- [ ] lazy.nvim-style plugin spec: `{ "id", enabled=bool, config = function() ... end }`
- [ ] Plugin Lua API: `petruterm.palette.register()`, `petruterm.on()`, `petruterm.notify()`
- [ ] Plugin event system: `tab_created`, `tab_closed`, `pane_split`, `ai_response`, `command_run`
- [ ] `petruterm.plugins.install("user/repo")` — git clone helper
- [ ] Plugin hot-reload (re-source plugin file on change)
- [ ] Example plugin + documentation

### Exit Criteria
A third-party Lua plugin can register a command palette action and a status bar widget. Plugin hot-reload works. `install()` clones a repo into the plugins directory.
