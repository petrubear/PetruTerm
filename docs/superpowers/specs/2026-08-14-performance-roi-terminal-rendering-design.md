# Performance ROI Terminal Rendering Design

## Goal

Improve interactive terminal latency and sustained throughput for large PTY
output by removing duplicated event-loop work, propagating damage at row
granularity, and uploading only changed GPU ranges.

## Scope

This first performance slice includes:

- Baseline instrumentation for the PTY, event-loop, frame, and GPU paths.
- Coalesced PTY polling, wakeups, and redraw requests.
- Explicit row revisions and invalidation reasons.
- Incremental CPU instance assembly for damaged rows.
- Persistent GPU instance buffers with coalesced partial uploads.
- Regression tests and a full-rebuild fallback for unsafe states.

This slice excludes:

- LLM streaming and context preparation.
- Glyph-atlas redesign.
- Search algorithm changes.
- User-visible configuration changes.
- Broad renderer refactoring unrelated to incremental updates.

## Current Problem

The critical path is:

`PTY reader -> poll_pty_events() -> collect_grid_cells_for() ->
build_instances() -> queue.write_buffer() -> render()`

Existing row and shaping caches prevent some expensive work, but clean rows are
still traversed, hashed, copied, and incorporated into frame-wide vectors.
Normal, LCD, and UI instance buffers are uploaded as complete buffers. PTY
events and redraw requests can also be polled or emitted more than once during
one event-loop cycle.

## Proposed Design

### 1. Instrument the existing path

Add tracing spans and counters around:

- PTY read, parse, and wakeup.
- `poll_pty_events()`.
- `collect_grid_cells_for()`.
- `build_instances()`.
- GPU upload byte counts and range counts.
- acquire, render, and present.
- input-to-pixel timing and event-loop wakeups.

Use repeatable scenarios for idle, interactive typing, ordinary output, large
output, scrolling, and resize.

### 2. Coalesce event-loop work

The PTY reader sets a pending flag and wakes the event loop. The event loop
drains pending PTY data once per iteration and collects all affected terminal
IDs in an efficient set. Redraw requests are coalesced so one iteration
produces at most one redraw request per window.

Output processing must retain a bounded batch duration or byte budget so a
high-volume process cannot starve keyboard input and window events.

### 3. Propagate row revisions

Each terminal row receives a content/style revision. The revision changes when
terminal content, colors, selection, search highlights, cursor state, or
relevant overlays change.

`collect_grid_cells_for()` and `RenderContext::build_instances()` consume
these revisions instead of recomputing hashes and resolved cell data for every
visible row. Clean row cache entries remain reusable with pane offsets applied
without rebuilding the final frame representation.

Global invalidation is explicit for resize, font changes, theme changes, and
atlas eviction.

### 4. Upload changed ranges

`GpuRenderer` owns persistent instance buffers. The CPU produces changed row
ranges, merges adjacent ranges, and writes only those ranges with
`queue.write_buffer()`.

The design retains a full-upload path for:

- buffer growth or resize;
- font, theme, or layout changes;
- atlas generation changes that invalidate cached vertices;
- device loss or any bounds/invariant failure.

The incremental path must never silently omit a row. A detected inconsistency
marks the affected pane for a full rebuild and surfaces an appropriate log
message.

## Data Flow

1. A PTY read produces terminal damage and sets the pending flag.
2. The event loop drains pending terminal events once.
3. The mux returns affected terminals and row revisions.
4. The renderer rebuilds only changed CPU row instances.
5. Adjacent changed rows become coalesced GPU upload ranges.
6. The renderer updates persistent buffers and presents the frame.
7. Cursor blink or other animation invalidates only its required rows.

## Error Handling and Safety

- Preserve the current full-frame path as a correctness fallback.
- Validate row/range bounds before issuing GPU writes.
- Treat resize, theme/font changes, atlas eviction, and device loss as
  explicit full-rebuild events.
- Do not drop PTY data when applying batch limits; leave remaining data
  pending for the next iteration.
- Preserve existing terminal locking and error propagation semantics.
- Avoid broad catches or success-shaped fallbacks.

## Testing Strategy

Add focused tests for:

- PTY wakeup and redraw coalescing.
- Terminal-ID deduplication.
- Row revision transitions.
- Cursor, selection, overlay, resize, theme, font, and atlas invalidation.
- Merging adjacent upload ranges.
- Full-rebuild fallback conditions.
- Equivalence between incremental and full-frame instance output.

Extend Criterion coverage for:

- One dirty row.
- A small contiguous dirty range.
- Scattered dirty rows.
- Full-screen damage.
- Small and large pane counts.

Validate with idle, interactive, high-output, scroll, resize, and multi-pane
runtime traces. Compare p50/p95 input-to-pixel latency, CPU time, wakeups,
bytes uploaded per frame, frame time, and output throughput.

## Task 7 Measurement Record

Instrumentation now labels each debug/HUD frame as `idle`, `interactive`,
`pty_output`, `scroll`, `resize`, or `multi_pane`. It reports the observed
incremental terminal/LCD upload bytes and ranges alongside the equivalent
full-buffer byte/range count; overlay and rectangle uploads remain in the
existing total upload counter. No threshold was changed: the existing 4 ms
PTY coalescing window, 8 ms echo poll, and batch limits were not retuned
because this environment did not provide an equivalent interactive runtime
trace.

| Machine / profile | Scenario | Baseline p50/p95 latency | Incremental p50/p95 latency | Baseline upload bytes | Incremental upload bytes | Throughput | Regressions / limitations |
|---|---|---:|---:|---:|---:|---:|---|
| Apple M4 Max, 14 CPU cores, 32-core GPU, 36 GB; macOS 26.5.2; release/optimized | idle, focused | N/A | N/A | N/A | N/A | N/A | GUI timing, wakeups/sec, and unchanged-frame writes were not safely observable from this non-interactive CLI session. |
| Same | interactive typing/paste | N/A | N/A | N/A | N/A | N/A | No keyboard/paste sample could be driven equivalently; the debug path retains the existing 4 ms/8 ms latency safeguards. |
| Same | 10,000-line PTY output | N/A | N/A | N/A | N/A | N/A | No controlled PTY producer/window trace was run; no throughput or CPU speedup is claimed. |
| Same | scroll / resize / multi-pane | N/A | N/A | N/A | N/A | N/A | GUI scroll, resize, font-scale, and pane movement measurements were unavailable. |
| Same; Criterion release bench | 80×24, one dirty row | N/A | N/A | 307,200 B / 1 range-pair | 12,800 B / 1 range-pair | N/A | Deterministic byte accounting only; it is not a GPU-device throughput measurement. |
| Same; Criterion release bench | 80×24, eight dirty rows | N/A | N/A | 307,200 B / 1 range-pair | 102,400 B / 1 merged range | N/A | Adjacent row ranges are coalesced by production upload planning. |
| Same; Criterion release bench | 80×24, full damage | N/A | N/A | 307,200 B / 1 range-pair | 307,200 B / 1 range-pair | N/A | Full-damage equivalence is intentional; no incremental advantage is claimed. |

The current Criterion observations were: `build_frame_miss` 34.561–34.740
µs, `build_frame_hit` 787.63–789.12 ns, one dirty row 882.11–887.17 ns,
eight contiguous rows 11.623–11.682 µs, scattered rows 9.3087–9.3502 µs,
full damage 34.987–35.157 µs, large serial hits 10.239–10.301 µs, and large
parallel hits 100.06–113.73 µs. These are current measurements, not an
equivalent before/after runtime baseline; repository Criterion change
annotations therefore are not treated as a Task 7 speedup claim.

Upload accounting benchmarks call the same production `UploadRange` merge and
accounting seam with explicit LCD-disabled and LCD-enabled inputs; they do not
assume a second buffer when LCD is disabled. These remain byte-accounting
benchmarks rather than GPU-device throughput measurements.

The HUD and `RUST_LOG=petruterm=debug` path now expose scenario, redraw,
user-event, and event-loop-wait counts, dirty/rebuilt rows, PTY byte volume,
actual upload bytes/ranges, and comparable full versus incremental terminal/LCD
counters. GUI/device values, input latency
percentiles, output throughput, CPU utilization, and visible-tearing checks
remain N/A until a controlled interactive run can collect them.

## Execution Tasks

1. Add baseline metrics and repeatable performance scenarios.
2. Coalesce PTY polling, wakeups, terminal-ID collection, and redraw requests.
3. Define row revision and invalidation contracts.
4. Convert CPU instance assembly to row-incremental updates.
5. Add persistent GPU buffers and coalesced `UploadRange` updates.
6. Add equivalence tests and full-rebuild fallback coverage.
7. Re-run benchmarks and traces, then tune thresholds.

Tasks 2 and 3 can be designed in parallel, but implementation follows the
listed order. Tasks 4 and 5 are the highest expected performance return; task
2 is the quickest lower-risk latency improvement.

## Success Criteria

- Lower p95 input-to-pixel latency under normal and high-output workloads.
- Fewer event-loop wakeups and no unnecessary redraw loop while idle.
- GPU upload bytes scale with damaged rows rather than the full visible grid.
- Higher sustained output throughput without starving input processing.
- Incremental and full-frame rendering produce equivalent visible output.
- Existing behavior remains available through a tested full-rebuild fallback.

## Follow-up Work

After this slice is measured and stable, evaluate LLM token batching and
background context preparation, atlas paging/generation management, and search
specialization as separate performance slices.
