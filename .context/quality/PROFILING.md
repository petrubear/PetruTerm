# PetruTerm — Profiling Guide

## Criterion Benchmarks

Run all benchmarks:

```bash
cargo bench
```

Run a specific benchmark:

```bash
cargo bench --bench shaping
cargo bench --bench search
```

HTML reports are generated in `target/criterion/`. Open `target/criterion/report/index.html` in a browser to view results.

### Bench coverage

| Bench | Status | Notes |
|-------|--------|-------|
| `shaping` | ✅ | ASCII / ligatures / unicode / cached paths |
| `search` | ✅ | Synthetic grid proxy of `Mux::search_active_terminal` + `filter_matches` |
| `build_instances` | ❌ blocked | `RenderContext`+`Mux` acoplados a `winit::EventLoopProxy`; requiere extraer CPU path a función pura |
| `rasterize_to_atlas` | ❌ blocked | Requiere `&wgpu::Queue`; opciones: (a) bench sólo `swash_cache.get_image_uncached` + conversión RGBA, (b) wgpu headless adapter en el bench |

### Baselines (2026-04-16, M4 Max, release profile)

**search.rs** — grid sintético 80 cols × (40 screen + 10 000 scrollback):

| Bench | Tiempo |
|-------|--------|
| `search_cold/common_word_the` | 2.00 ms |
| `search_cold/common_word_error` | 1.81 ms |
| `search_cold/rare_word_zzz` | 2.17 ms |
| `search_cold/medium_case_Error` | 1.74 ms |
| `search_incremental_extend_e_to_error` | 153 µs |

El bench incremental corre ~12× más rápido que el cold scan; confirma empíricamente el valor de TD-PERF-11 (`filter_matches`).

**shaping.rs** — ver tabla en `.context/specs/build_phases.md` Sub-A.

To install the `cargo-criterion` CLI for richer output:

```bash
cargo install cargo-criterion
cargo criterion
```

## cargo flamegraph

Install:

```bash
cargo install flamegraph
# macOS: also install DTrace support (no extra steps needed on macOS)
```

Generate a flamegraph for the release binary:

```bash
cargo flamegraph --bin petruterm
```

The output SVG (`flamegraph.svg`) opens in any browser. Zoom into hot frames to identify bottlenecks.

To profile the shaping benchmark:

```bash
cargo flamegraph --bench shaping
```

## Instruments (macOS)

### Time Profiler

1. Build in release: `cargo build --release`
2. Open Instruments: `open -a Instruments`
3. Choose **Time Profiler** template
4. Target: `target/release/petruterm`
5. Press Record, use the terminal normally, press Stop
6. Filter the call tree by **petruterm** to remove system noise

### Metal System Trace

Use the **Metal System Trace** template to see GPU command encoder timing, render pass duration, and buffer upload costs. This is the most useful template for diagnosing frame-time spikes caused by the wgpu render loop.

### GPU Frame Capture (wgpu/Metal)

wgpu supports Metal GPU frame capture. Add to the launch environment:

```bash
METAL_DEVICE_WRAPPER_TYPE=1 cargo run --release
```

Then use Instruments **GPU Frame Capture** or Xcode's GPU debugger to inspect draw calls.

## Tracing / Tracy

Enable the `profiling` feature to activate `tracing` spans on hot paths:

```bash
cargo run --features profiling
```

Spans instrumented:
- `redraw_frame` — full `RedrawRequested` handler
- `build_instances` — per-pane cell instance generation (row cache hit/miss)
- `shape_line` — HarfBuzz text shaping via cosmic-text

To integrate with Tracy profiler:

1. Add `tracing-tracy` to `[dependencies]` (behind `profiling` feature)
2. Initialize the Tracy subscriber in `main.rs` when `cfg!(feature = "profiling")`
3. Run with Tracy client open to see live span timelines

## Debug HUD (F12)

Press **F12** at runtime to toggle the in-app debug overlay. It shows:

```
F12 HUD
frame      Xms  p50:Xms  p95:Xms
shape      hits=N miss=N (X%)
instances  N
atlas      X%
```

- **frame**: rolling average / p50 / p95 frame time over the last 120 frames
- **shape**: per-session row cache hit/miss ratio (resets on restart)
- **instances**: CellVertex count submitted to the GPU last frame
- **atlas**: glyph atlas fill percentage (eviction triggers at 90%)

Frame times > 16.67 ms (< 60 fps) are shown in red.
