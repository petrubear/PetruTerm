# PetruTerm — Build Phases

> Phases 0.5–3 (complete) archived in [`build_phases_archive.md`](./build_phases_archive.md).

---

## Phase 3.5: Performance Sprint ⚡
**Status: In progress — Sub-phases A/B/C/D/F complete**

| KPI | Target |
|-----|--------|
| Input-to-pixel latency p99 | < 8 ms |
| Input-to-pixel latency p50 | < 4 ms |
| Steady-state frame time | < 1 ms |
| Idle allocations | 0 |
| Cache-miss storm | < 16 ms |
| Startup (exec → first pixel) | < 80 ms |

---

### Sub-phase A: Measurement Infrastructure ✅ (parcial)

**Baselines (2026-04-14, M4 Max, release):**
| Benchmark | Antes | Despues (Sub-C) |
|-----------|-------|-----------------|
| `shape_line_ascii` | 5 643 ns | 317 ns (-94%) |
| `shape_line_ligatures` | 8 766 ns | 659 ns (-92%) |
| `shape_line_unicode` | 5 586 ns | ~5 700 ns (sin cambio) |

- [x] `benches/shaping.rs` con criterion
- [x] Tracing + feature flag `profiling`: spans en `build_instances`, `shape_line`, `RedrawRequested`
- [x] Debug HUD (F12): frame time p50/p95, shape cache hit%, atlas fill%, instance count, GPU upload KB/frame
- [x] `.context/quality/PROFILING.md`
- [ ] Frame budget en `term_specs.md`
- [ ] Benches adicionales: `build_instances`, `search_active_terminal`, `rasterize_to_atlas`
- [ ] CI gating: regresion > 5% falla build
- [ ] Tracy integration, GPU timestamps, os_signpost, latency probe completo

---

### Sub-phase B: Idle Zero-Cost ✅

- [x] `ControlFlow::Wait` cuando idle (sin PTY, sin overlay, sin drag)
- [x] Cursor blink pausado en idle
- [x] GPU upload bytes counter en HUD
- [x] `poll_git_branch` → timer 1 Hz independiente (TD-PERF-19)
- [x] In-flight guard en git branch fetch
- [ ] Cursor como overlay independiente (blink sin invalidar grid cache)
- [ ] Damage tracking con `Term::damage()`

---

### Sub-phase C: Hot Path Fast Paths ✅

- [x] Ligature scan bit: `bytes().any()` antes de HarfBuzz
- [x] ASCII fast path: skip HarfBuzz para ASCII sin ligature chars (317 ns vs 5 643 ns baseline)
- [x] Per-word shape cache: `HashMap<(u64,u32), ShapedRun>`, cap 512 entries
- [x] Space cell fast path: `' '` + default bg salta glyph pipeline
- [x] Row hash fix: hashea los 4 canales RGBA (bug: solo hasheaba canal rojo)
- [ ] Pre-shape warmup ASCII 32-126 al arranque
- [ ] Subpixel position quantization

---

### Sub-phase D: Memory & Allocator ✅ (parcial)

- [x] `mimalloc` como `#[global_allocator]`
- [x] Scratch buffers en `RenderContext`: `scratch_chars`, `scratch_str`, `scratch_colors`, `fmt_buf`
- [x] `ChatPanel` separator cached — `'─'.repeat(n)` solo en resize
- [ ] Bumpalo arena per-frame
- [ ] `smallvec` en hot paths
- [ ] `compact_str` para strings cortos

---

### Sub-phase E: Parallel Rendering

- [ ] Rayon per-pane parallel build
- [ ] Parallel row shaping en cache-miss storm
- [ ] Lock-free PTY ring buffer (`rtrb` SPSC)
- [x] PTY reader thread steered to efficiency cores via `QOS_CLASS_UTILITY` (OnceLock, once per thread)

---

### Sub-phase F: Latency Minimization ✅ (parcial)

- [x] `PresentMode::Mailbox → FifoRelaxed → Fifo` (auto por caps)
- [x] `desired_maximum_frame_latency: 2 → 1`
- [x] Adaptive PTY coalescing: <=2 eventos = redraw inmediato; >2 = 4ms window
- [x] Skip render cuando window ocluida (`WindowEvent::Occluded`)
- [x] Input-to-pixel latency probe (`RUST_LOG=petruterm=debug`)
- [ ] Input event priority sobre PTY en tick
- [ ] `CVDisplayLink` en macOS (experimental)
- [ ] `CAMetalLayer::setDisplaySyncEnabled(false)` (experimental)

---

### Sub-phase G: GPU Architecture

- [ ] Atlas split por tamano: 1024 ASCII + 4096 emoji/wide
- [ ] Persistent mapped ring buffer (3x frame in flight)
- [ ] Indirect draw para multi-pane
- [ ] Unificar bg + glyph en un solo pass
- [ ] GPU-resident grid (Phase 5+ candidate)

---

### Sub-phase H: Build & Release

- [ ] PGO con workload representativo
- [x] `target-cpu=apple-m1` en `bundle.sh`
- [x] `release-native` profile en `Cargo.toml` (`[profile.release-native]`)
- [x] Lua bytecode cache (`~/.cache/petruterm/lua-bc/*.luac`, mtime-validated)
- [ ] Config eager-load en paralelo con window creation

---

### Exit Criteria (Phase 3.5 global)

- [ ] Input-to-pixel latency: **p50 < 4 ms, p99 < 8 ms**
- [ ] Steady-state frame time: **< 1 ms**
- [ ] Zero allocations en idle (verificado con `dhat`)
- [ ] Cache-miss storm: **< 16 ms**
- [ ] Startup: **< 80 ms**
- [ ] Criterion benchmarks con CI gating
- [ ] Debug HUD (F12) operativo ✅
- [ ] `PROFILING.md` ✅
- [ ] Comparativa vs Alacritty/kitty/wezterm en `.context/quality/BENCHMARKS.md`

---

## Phase 4: Plugin Ecosystem
**Status: Not started — bloqueado hasta Phase 3.5 exit criteria**

- [ ] Plugin loader: auto-scan `~/.config/petruterm/plugins/*.lua`
- [ ] lazy.nvim-style plugin spec
- [ ] Plugin Lua API: `petruterm.palette.register()`, `petruterm.on()`, `petruterm.notify()`
- [ ] Plugin event system: `tab_created`, `tab_closed`, `pane_split`, `ai_response`, `command_run`
- [ ] `petruterm.plugins.install("user/repo")`
- [ ] Plugin hot-reload
- [ ] Example plugin + documentation
