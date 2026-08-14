# Terminal Performance ROI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce interactive terminal latency and improve sustained high-volume PTY output throughput by coalescing event-loop work, propagating row damage, and uploading only changed GPU ranges.

**Architecture:** Keep the existing full-frame renderer as a correctness fallback while adding a measured incremental path. A PTY wakeup gate coalesces notifications, `Mux` reports dirty rows, `RenderContext` stores stable per-row CPU slots, and `GpuRenderer` writes merged ranges into persistent vertex buffers. Dynamic cursor/UI overlays remain separate from persistent terminal instances so clean terminal rows are never copied into a new frame-wide vector.

**Tech Stack:** Rust 2021, wgpu 29, winit 0.30, alacritty_terminal 0.25, crossbeam-channel 0.5, Criterion 0.5, optional tracing/Tracy profiling, parking_lot, rustc-hash.

## Global Constraints

- Preserve the current full-frame path as a correctness fallback.
- The incremental path must never silently omit a row.
- Do not drop PTY data when applying batch limits; leave remaining data pending for the next iteration.
- Treat resize, theme/font changes, atlas eviction, and device loss as explicit full-rebuild events.
- No user-visible configuration changes.
- Do not add dependencies; use existing `tracing`, `crossbeam-channel`, `rustc-hash`, wgpu, and Criterion facilities.
- Keep GPU work on the main thread and retain existing terminal locking/error propagation semantics.
- Do not apply Rayon to small frame assembly; the existing benchmark measured its fork-join path as slower than serial work.

---

## File Map

| File | Responsibility in this plan |
|---|---|
| `src/app/perf.rs` | Pure frame/work counters and reset helpers that can be unit-tested without a window. |
| `src/app/pty_schedule.rs` | Lock-free PTY wakeup gate and deterministic coalescing helpers. |
| `src/app/mod.rs` | Owns the pending PTY batch, event-loop scheduling, and the single drain point. |
| `src/app/frame.rs` | Consumes the pending PTY batch, builds panes, and submits persistent/range uploads. |
| `src/app/mux/mod.rs` | Drains terminal channels once, deduplicates terminal IDs, and emits row damage metadata. |
| `src/app/renderer/damage.rs` | Pure dirty-row representation, range conversion, and row revision bookkeeping. |
| `src/app/renderer/layout.rs` | Stable terminal row slots and bounded CPU instance storage. |
| `src/app/renderer/mod.rs` | Owns row revisions, terminal instance storage, upload ranges, and dynamic overlay buffers. |
| `src/app/renderer/terminal.rs` | Rebuilds only dirty rows and writes their local vertices into stable slots. |
| `src/renderer/upload.rs` | Pure `UploadRange` type and adjacent-range merging. |
| `src/renderer/gpu.rs` | Persistent terminal/LCD buffers, range writes, overlay buffer, and draw ordering. |
| `src/term/pty.rs` | Uses the shared wakeup gate for reader, parser, and child-exit notifications. |
| `benches/build_instances.rs` | Synthetic dirty-row, dirty-range, and full-damage comparisons. |

The plan is one vertical slice rather than separate subsystem projects: every
task ends with a testable deliverable, and the PTY, damage, CPU, and GPU
changes are wired together only after their pure contracts exist.

### Task 1: Adding baseline performance counters

**Files:**
- Create: `src/app/perf.rs`
- Modify: `src/app/mod.rs`
- Modify: `src/app/frame.rs`
- Modify: `src/app/mux/mod.rs`
- Modify: `src/app/renderer/mod.rs`
- Modify: `src/renderer/gpu.rs`
- Modify: `benches/build_instances.rs`
- Test: `src/app/perf.rs` unit tests

**Interfaces:**
- Produces `pub(crate) struct FrameMetrics` with counters for PTY terminals, dirty rows, rebuilt rows, upload ranges, upload bytes, and wakeups.
- Produces `FrameMetrics::reset()` and `FrameMetrics::record_upload(bytes, range_count)`.
- Existing HUD fields `frame_times`, `latency_samples`, and `last_gpu_upload_bytes` remain compatible; the new counters add observability rather than changing their meaning.

- [ ] **Step 1: Write the failing metrics tests**

Add tests in `src/app/perf.rs` for reset and upload accumulation:

```rust
#[test]
fn upload_metrics_accumulate_and_reset() {
    let mut metrics = FrameMetrics::default();
    metrics.record_upload(128, 2);
    metrics.record_upload(64, 1);

    assert_eq!(metrics.upload_bytes, 192);
    assert_eq!(metrics.upload_ranges, 3);

    metrics.reset();
    assert_eq!(metrics, FrameMetrics::default());
}
```

Run:

```bash
cargo test --lib upload_metrics_accumulate_and_reset
```

Expected: FAIL because `src/app/perf.rs` and `FrameMetrics` do not exist.

- [ ] **Step 2: Implement the pure counter type**

Create `src/app/perf.rs`:

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FrameMetrics {
    pub pty_terminals: usize,
    pub dirty_rows: usize,
    pub rebuilt_rows: usize,
    pub upload_ranges: usize,
    pub upload_bytes: usize,
    pub wakeups: usize,
}

impl FrameMetrics {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn record_upload(&mut self, bytes: usize, ranges: usize) {
        self.upload_bytes = self.upload_bytes.saturating_add(bytes);
        self.upload_ranges = self.upload_ranges.saturating_add(ranges);
    }
}
```

Register the module from `src/app/mod.rs`.

- [ ] **Step 3: Attach counters to existing frame ownership**

Add `pub(crate) frame_metrics: FrameMetrics` to `RenderContext`, initialize it
in `RenderContext::new`, and reset it at the start of `begin_frame`.

Increment:

- `pty_terminals` when a drained batch contains a unique terminal ID.
- `dirty_rows` when `collect_grid_cells_for` reports rows.
- `rebuilt_rows` when `build_instances` actually reshapes a row.
- `wakeups` in the single `App::user_event` path.
- `upload_bytes` and `upload_ranges` from actual GPU writes, not from the length of the logical frame vectors.

- [ ] **Step 4: Add profiling spans without changing behavior**

Use the existing optional profiling feature and add spans around the hot
boundaries:

```rust
#[cfg(feature = "profiling")]
let _span = tracing::info_span!(
    "poll_pty_events",
    terminal_count = self.terminals.len()
)
.entered();
```

Add equivalent spans named `collect_grid_cells`, `build_instances`, and
`upload_instance_ranges`. Keep the existing `redraw_frame`, `shape_line`, and
`rasterize_to_atlas` spans.

- [ ] **Step 5: Extend the synthetic benchmark with metric-shaped cases**

Add benchmark functions to `benches/build_instances.rs` for one dirty row,
eight contiguous dirty rows, scattered dirty rows, and all 24 rows. Reuse the
existing `SAMPLE_ROWS`, `build_row_vertices`, and `apply_row_offset` helpers so
the new measurements remain comparable with the current 0.13 microsecond row
hit and 10 microsecond 200-row serial results.

- [ ] **Step 6: Run the baseline checks**

Run:

```bash
cargo fmt --check
cargo test --lib
cargo bench --bench build_instances -- --noplot --warm-up-time 1 --measurement-time 2 --sample-size 10
cargo check --features profiling
```

Expected: all existing tests pass, the benchmark reports all existing and new
cases, and the profiling feature still compiles.

- [ ] **Step 7: Commit the baseline instrumentation**

```bash
git add src/app/perf.rs src/app/mod.rs src/app/frame.rs src/app/mux/mod.rs \
  src/app/renderer/mod.rs src/renderer/gpu.rs benches/build_instances.rs
git commit -m "[PERF-ROI-01] feat: Add performance baseline counters." \
  -m "Expose PTY, row rebuild, upload range, and wakeup counters before changing scheduling or rendering behavior." \
  -m "Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

### Task 2: Coalescing PTY wakeups and event-loop drains

**Files:**
- Create: `src/app/pty_schedule.rs`
- Modify: `src/app/mod.rs`
- Modify: `src/app/frame.rs`
- Modify: `src/app/mux/mod.rs`
- Modify: `src/term/pty.rs`
- Test: `src/app/pty_schedule.rs` unit tests

**Interfaces:**
- Produces `pub(crate) struct WakeupGate` backed by `Arc<AtomicBool>`.
- Produces `WakeupGate::signal() -> bool`, where `true` means the caller must send one `EventLoopProxy` event.
- Produces `WakeupGate::clear_and_rearm(has_pending: bool) -> bool`, which clears before checking pending state and returns whether another wakeup must be sent.
- Produces `pub struct PtyEventBatch { pub data_ids: Vec<usize>, pub exited: Vec<usize>, pub has_pending: bool }`.
- Changes `Mux::poll_pty_events` to return `PtyEventBatch`.

- [ ] **Step 1: Write deterministic wakeup-gate tests**

Add tests for one signal per pending period and rearming after a drain:

```rust
#[test]
fn gate_sends_once_until_rearmed() {
    let gate = WakeupGate::new();
    assert!(gate.signal());
    assert!(!gate.signal());
    assert!(gate.clear_and_rearm(true));
    assert!(!gate.signal());
}

#[test]
fn gate_rearms_when_drain_observes_no_pending_work() {
    let gate = WakeupGate::new();
    assert!(gate.signal());
    assert!(!gate.clear_and_rearm(false));
    assert!(gate.signal());
}
```

Run:

```bash
cargo test --lib gate_
```

Expected: FAIL because the gate does not exist.

- [ ] **Step 2: Implement the atomic gate**

Implement the clear-before-recheck protocol:

```rust
pub(crate) fn clear_and_rearm(&self, has_pending: bool) -> bool {
    self.pending.store(false, Ordering::Release);
    has_pending && !self.pending.swap(true, Ordering::AcqRel)
}
```

The caller sends an event only when `signal()` or `clear_and_rearm()` returns
`true`. This ordering prevents an event that arrives during the drain from
being hidden behind a stale `true` flag.

- [ ] **Step 3: Share the gate with every PTY producer**

Add one `Arc<WakeupGate>` to `Mux`, pass it through terminal creation into
`Pty::spawn`, and store it in `PtyEventProxy`.

Update `PtyEventProxy::send_event`, `reader_loop`, and the child-monitor
thread so they still enqueue the original `PtyEvent` but call
`wakeup.send_event(())` only after the gate permits it. Do not remove events
from the bounded channel and do not turn `DataReady` into a data-carrying
event; the terminal grid remains the source of truth.

- [ ] **Step 4: Return one deduplicated batch from `Mux`**

Change `poll_pty_events` to accumulate `PtyEventBatch`. Replace
`Vec::contains()` for terminal and exit IDs with per-drain `Vec<bool>` arrays
indexed by terminal ID. Preserve all existing handling for clipboard,
OSC-133, screen clear, exit codes, and disconnected channels.

Add `Mux::has_pending_pty_events() -> bool` using each live terminal's
crossbeam receiver `is_empty()` state. This is used only for the gate
re-arm check after the drain.

- [ ] **Step 5: Make `about_to_wait` the single normal drain point**

Add App fields:

```rust
pending_pty_batch: Option<PtyEventBatch>,
pty_wakeup_pending: bool,
```

In `user_event`, increment the wakeup metric and set `pty_wakeup_pending` but
do not call `poll_pty_events`.

In `about_to_wait`, call `poll_pty_events` at most once when a wakeup is
pending, the PTY coalescing deadline expires, or the safety poll is active.
Store the returned batch, update shell context once per unique `data_id`, and
set `pending_pty_redraw` without immediately polling again.

In `handle_redraw`, consume the stored batch with `take()` and remove its
direct call to `poll_pty_events`. Preserve the existing `close_exited_terminals`
and `apply_osc133_events` ordering.

- [ ] **Step 6: Preserve latency windows while eliminating duplicate work**

Keep `PTY_ECHO_GRACE_MS = 250`, `PTY_ECHO_GRACE_POLL_MS = 8`,
`PTY_SAFETY_POLL_MS = 3000`, and `PTY_SAFETY_POLL_INTERVAL_MS = 100`.

Use the timers to request a drain, not to perform an extra drain from
`handle_redraw`. When a batch is drained, clear the gate and immediately
re-arm it if `Mux::has_pending_pty_events()` reports new channel data.

- [ ] **Step 7: Run scheduling tests and checks**

Run:

```bash
cargo fmt --check
cargo test --lib gate_
cargo test --lib
cargo check --features profiling
```

Expected: gate tests pass, all existing library tests pass, and no PTY event
producer loses its channel event or wakeup path.

- [ ] **Step 8: Commit the scheduling slice**

```bash
git add src/app/pty_schedule.rs src/app/mod.rs src/app/frame.rs \
  src/app/mux/mod.rs src/term/pty.rs
git commit -m "[PERF-ROI-01] fix: Coalesce PTY wakeups and drains." \
  -m "Drain PTY channels once per event-loop cycle while preserving bounded echo and safety polling." \
  -m "Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

### Task 3: Propagating explicit dirty rows and revisions

**Files:**
- Create: `src/app/renderer/damage.rs`
- Modify: `src/app/mux/mod.rs`
- Modify: `src/app/frame.rs`
- Modify: `src/app/renderer/mod.rs`
- Modify: `src/app/renderer/terminal.rs`
- Test: `src/app/renderer/damage.rs` unit tests

**Interfaces:**
- Produces `RowRange { start: usize, end: usize }` with an exclusive `end`.
- Produces `DirtyRows` with `mark(row)`, `mark_range(start, end)`, `mark_all(row_count)`, `is_dirty(row)`, `is_full()`, `len()`, and `ranges(row_count)`.
- Produces `RowRevisionMap` with `mark(row)`, `mark_all(row_count)`, and `revision(row) -> u64`.
- Changes `Mux::collect_grid_cells_for` to accept `&mut DirtyRows` and report the rows it actually refreshed or force-invalidated.
- Adds `revision: u64` to `RowCacheEntry`.

- [ ] **Step 1: Write dirty-range and revision tests**

Add tests covering deduplication, adjacent range merging, full damage, and
monotonic revisions:

```rust
#[test]
fn dirty_rows_merge_and_sort_ranges() {
    let mut rows = DirtyRows::default();
    rows.mark(5);
    rows.mark_range(1, 3);
    rows.mark(3);
    rows.mark(4);

    assert_eq!(
        rows.ranges(8),
        vec![RowRange { start: 1, end: 6 }]
    );
}

#[test]
fn full_damage_covers_requested_rows() {
    let mut rows = DirtyRows::default();
    rows.mark_all(4);

    assert!(rows.is_full());
    assert!((0..4).all(|row| rows.is_dirty(row)));
}

#[test]
fn row_revisions_increase_only_for_marked_rows() {
    let mut revisions = RowRevisionMap::default();
    revisions.mark(2);
    let first = revisions.revision(2);
    revisions.mark(4);

    assert!(revisions.revision(2) == first);
    assert!(revisions.revision(4) > first);
}
```

Run:

```bash
cargo test --lib dirty_rows
```

Expected: FAIL because the damage module does not exist.

- [ ] **Step 2: Implement the pure damage types**

Create `src/app/renderer/damage.rs`. Store explicitly marked rows in a
deduplicated vector plus a `full` flag. `ranges(row_count)` must sort,
deduplicate, clamp to `row_count`, and merge adjacent rows. A full set returns
one range from zero to `row_count`.

`RowRevisionMap::mark` increments one monotonically wrapping counter and stores
the resulting value for the row. `mark_all` resizes the row vector and assigns
a new value to every row. The renderer uses the revision as the primary cache
key and retains the existing hash as a defensive content check.

- [ ] **Step 3: Thread damage out of `collect_grid_cells_for`**

Add `dirty_rows: &mut DirtyRows` to the existing
`Mux::collect_grid_cells_for` signature.

At the start of the function, clear the output damage set. Mark all rows when
`force_full`, `TermDamage::Full`, a terminal switch, or an invalid row-cache
state requires a full read. For `TermDamage::Partial`, mark only the returned
line numbers when selection and search are inactive. Continue to read
selection/search frames fully because their colors depend on cells outside the
terminal damage set.

Always mark the ghost-text row and flag-hint row. Mark cursor rows when cursor
state changes. Keep stale `buf` entries for clean rows so the renderer can
reuse their cache entries without reading the grid again.

- [ ] **Step 4: Store revision state per terminal**

Add `row_revisions: HashMap<usize, RowRevisionMap>` to `RenderContext`.
Initialize and clear it with `row_caches`. Add `revision: u64` to
`RowCacheEntry`.

In `build_all_pane_instances`, create or retrieve the terminal's
`DirtyRows`, pass it to both collection and instance building, and mark all
rows when `scratch_terminal_id` changes or a row cache has no entry.

- [ ] **Step 5: Make cache hits skip color resolution and hashing**

Change `RenderContext::build_instances` to accept `&DirtyRows`.

For a clean row with an existing `RowCacheEntry` at the current row revision,
skip `resolve_color`, style copying, and `calculate_row_hash`. For a dirty row,
resolve colors, calculate the hash, shape/rasterize on a hash miss, and store
the current revision. If a clean row lacks a cache entry, mark it dirty and
perform the normal rebuild instead of reading uninitialized storage.

- [ ] **Step 6: Test the invalidation matrix**

Add unit tests for the pure damage module and renderer tests for:

- a partial PTY damage set rebuilding only the listed rows;
- a clean row with an existing cache entry avoiding color/hash work;
- cursor, selection, search, ghost, and flag-hint rows becoming dirty;
- resize/font/theme changes calling full invalidation;
- a terminal switch forcing all rows to be read.

Use a counter on `RenderContext` to assert the expected rebuilt-row count
instead of relying on timing.

- [ ] **Step 7: Run the damage tests**

Run:

```bash
cargo fmt --check
cargo test --lib dirty_rows
cargo test --lib
cargo check --features profiling
```

Expected: pure damage tests and the existing library suite pass, with no
change to visible behavior because GPU uploads still use the old full path at
this point.

- [ ] **Step 8: Commit the damage contract**

```bash
git add src/app/renderer/damage.rs src/app/renderer/mod.rs \
  src/app/renderer/terminal.rs src/app/mux/mod.rs src/app/frame.rs
git commit -m "[PERF-ROI-01] feat: Propagate terminal row damage." \
  -m "Use explicit row revisions to skip clean-grid color resolution, hashing, and shaping work." \
  -m "Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

### Task 4: Storing terminal instances in stable row slots

**Files:**
- Create: `src/app/renderer/layout.rs`
- Modify: `src/app/renderer/mod.rs`
- Modify: `src/app/renderer/terminal.rs`
- Modify: `src/app/frame.rs`
- Test: `src/app/renderer/layout.rs` unit tests

**Interfaces:**
- Produces `RowSlot { start: usize, capacity: usize, len: usize }`.
- Produces `TerminalInstanceLayout { terminal_id, columns, rows, col_offset, row_offset, row_stride, slots }`.
- Produces `TerminalInstanceLayout::rebuild(...)`, `row_slot(row)`, and `write_row(...)`.
- `RenderContext` owns persistent `terminal_instances`, `terminal_lcd_instances`, per-terminal layouts, and pending cell/LCD upload ranges.
- `build_instances` updates persistent slots and returns no frame-wide terminal vector.

- [ ] **Step 1: Write slot-layout tests**

Add tests for non-overlapping row slots, row writes with transparent padding,
and layout invalidation when dimensions or pane origin change:

```rust
#[test]
fn row_slots_are_non_overlapping_and_bounded() {
    let layout = TerminalInstanceLayout::rebuild(7, 80, 24, 0, 0, 160);
    let first = layout.row_slot(0).unwrap();
    let second = layout.row_slot(1).unwrap();

    assert_eq!(first.start + first.capacity, second.start);
    assert_eq!(layout.rows, 24);
}
```

Run:

```bash
cargo test --lib row_slots_are_non_overlapping_and_bounded
```

Expected: FAIL because the layout module does not exist.

- [ ] **Step 2: Implement bounded row slots**

Create `src/app/renderer/layout.rs`.

Use a row stride of `max(columns.saturating_mul(2), 1)` for the common
background-plus-glyph case. `write_row` must:

1. validate the target row and vertex count;
2. copy the row's global-position vertices into the slot;
3. fill unused capacity with transparent zero-area vertices;
4. return an error when the row exceeds capacity.

Do not truncate an over-capacity row. The caller must rebuild the layout with a
larger stride based on the observed row size or select the full-frame fallback.

- [ ] **Step 3: Add persistent renderer storage**

In `RenderContext`, add:

```rust
pub(crate) terminal_instances: Vec<CellVertex>,
pub(crate) terminal_lcd_instances: Vec<CellVertex>,
pub(crate) terminal_layouts: HashMap<usize, TerminalInstanceLayout>,
pub(crate) instance_upload_ranges: Vec<UploadRange>,
pub(crate) lcd_upload_ranges: Vec<UploadRange>,
```

Change `begin_frame` so it clears only dynamic overlay/rectangle vectors and
the per-frame range lists. It must not clear persistent terminal storage or
row caches.

- [ ] **Step 4: Prepare layouts before pane collection**

Add `RenderContext::prepare_terminal_layouts(&[PaneInfo])`. Rebuild a
terminal's layout when its visible row count, column count, pane origin, or
row stride changes. Mark every row in that layout dirty after a rebuild.

When a pane disappears, remove its layout and its row cache. When a terminal
is reused at a new pane origin, invalidate all rows because the cached global
positions are no longer valid.

- [ ] **Step 5: Rewrite `build_instances` around dirty slots**

Keep the existing serial shaping/rasterization code for dirty rows, but:

- skip all phase-one work for clean rows with matching revisions;
- store shaped vertices in `RowCacheEntry` using local pane coordinates;
- call `TerminalInstanceLayout::write_row` for dirty rows;
- append the corresponding cell and LCD slot ranges to the renderer range lists;
- increment `rebuilt_rows` only when a row is shaped or rewritten;
- retry once with a larger row stride when a row exceeds capacity;
- request a full layout rebuild when the retry would exceed the GPU capacity.

Do not run Rayon in this path. The existing 200-row benchmark showed the
parallel offset-copy path at about 121 microseconds versus about 10
microseconds serially.

- [ ] **Step 6: Move cursor and dynamic UI instances out of terminal storage**

Keep cursor, command palette, context menu, info overlay, toast, and HUD
instances in a per-frame `overlay_instances` vector. Preserve ordering by
emitting the cursor first, then the existing UI overlay builders in their
current order.

Update `build_cursor_instance` so cursor blink changes only the first overlay
slot. The terminal LCD rows no longer need to be patched by cursor position
because the cursor overlay is drawn after terminal glyphs.

- [ ] **Step 7: Run layout and renderer tests**

Run:

```bash
cargo fmt --check
cargo test --lib row_slots
cargo test --lib
cargo check --features profiling
```

Expected: persistent CPU storage produces the same logical instances as the
old frame-wide vector, and the full upload API is still used until Task 5
wires range uploads.

- [ ] **Step 8: Commit stable CPU slots**

```bash
git add src/app/renderer/layout.rs src/app/renderer/mod.rs \
  src/app/renderer/terminal.rs src/app/frame.rs
git commit -m "[PERF-ROI-01] feat: Store terminal instances in row slots." \
  -m "Keep clean terminal rows in persistent CPU storage and expose only changed slot ranges to the GPU layer." \
  -m "Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

### Task 5: Adding merged GPU range uploads

**Files:**
- Create: `src/renderer/upload.rs`
- Modify: `src/renderer/mod.rs`
- Modify: `src/renderer/gpu.rs`
- Modify: `src/app/renderer/mod.rs`
- Modify: `src/app/frame.rs`
- Test: `src/renderer/upload.rs` unit tests

**Interfaces:**
- Produces `UploadRange { start: usize, end: usize }` with an exclusive `end`.
- Produces `merge_upload_ranges(&mut [UploadRange]) -> Vec<UploadRange>`, which sorts, removes empty ranges, and merges overlapping or adjacent ranges. The GPU caller validates the returned ranges against storage bounds.
- Produces `GpuRenderer::upload_instance_ranges(&[CellVertex], &[UploadRange]) -> Result<usize>`.
- Produces `GpuRenderer::upload_lcd_ranges(&[CellVertex], &[UploadRange]) -> Result<usize>`.
- Produces `GpuRenderer::upload_overlay_instances(&[CellVertex]) -> Result<usize>`.
- Replaces `set_cell_count`/`set_overlay_start` with explicit terminal and overlay counts while retaining compatibility wrappers only until all call sites migrate.

- [ ] **Step 1: Write range-merging tests**

Add tests for empty, adjacent, overlapping, and scattered ranges:

```rust
#[test]
fn merge_upload_ranges_coalesces_adjacent_and_overlapping_ranges() {
    let mut input = vec![
        UploadRange { start: 8, end: 12 },
        UploadRange { start: 0, end: 4 },
        UploadRange { start: 4, end: 8 },
        UploadRange { start: 10, end: 14 },
    ];

    assert_eq!(
        merge_upload_ranges(&mut input),
        vec![UploadRange { start: 0, end: 14 }]
    );
}
```

Run:

```bash
cargo test --lib merge_upload_ranges_coalesces_adjacent_and_overlapping_ranges
```

Expected: FAIL because the range module does not exist.

- [ ] **Step 2: Implement the pure range helper**

Create `src/renderer/upload.rs` with `UploadRange` deriving
`Clone, Copy, Debug, Eq, PartialEq`. Sort by `(start, end)`, discard
`start >= end`, and merge whenever `next.start <= current.end`.

Add the module to `src/renderer/mod.rs` and re-export the type for
`src/app/renderer`.

- [ ] **Step 3: Add persistent GPU write APIs**

In `GpuRenderer`, add range-write methods that:

1. validate each range against both the CPU slice and GPU buffer capacity;
2. write only the byte interval
   `start * size_of::<CellVertex>() .. end * size_of::<CellVertex>()`;
3. accumulate the actual bytes and range count for `FrameMetrics`;
4. return an error on an invalid range rather than logging a success-shaped fallback.

Retain the existing full upload method as a wrapper that submits one range
covering the complete slice. This keeps the full-frame fallback and makes the
old behavior directly comparable.

- [ ] **Step 4: Separate persistent terminal and dynamic overlay buffers**

Add an `overlay_instance_buffer` and its count to `GpuRenderer`. The existing
terminal instance buffer becomes persistent storage for row slots. Keep LCD
storage persistent as well.

Implement:

```rust
pub fn set_terminal_cell_count(&mut self, count: usize);
pub fn set_overlay_count(&mut self, count: usize);
pub fn upload_overlay_instances(
    &mut self,
    instances: &[CellVertex],
) -> anyhow::Result<usize>;
```

Allocate the overlay buffer with the existing `MAX_INSTANCES` capacity and
return a clear error when the count exceeds it.

- [ ] **Step 5: Preserve draw ordering in `render`**

Update `GpuRenderer::render` so the terminal pass binds the persistent
terminal buffer for the terminal draw, binds the LCD buffer for LCD glyphs,
then binds the overlay buffer for cursor and UI overlay draws.

Keep rounded rectangles before cell backgrounds. Keep the cursor first in
the overlay vector so command palette/context menu backgrounds still cover
terminal text in the same order as before.

- [ ] **Step 6: Wire ranges through `handle_redraw`**

Replace the current sequence:

```rust
rc.renderer.upload_rect_instances(&rc.rect_instances);
rc.renderer.set_overlay_start(overlay_start);
rc.renderer.upload_instances(&rc.instances, 0);
rc.renderer.set_cell_count(rc.instances.len());
rc.renderer.upload_lcd_instances(&rc.lcd_instances);
```

with:

```rust
let cell_ranges = merge_upload_ranges(&mut rc.instance_upload_ranges);
let lcd_ranges = merge_upload_ranges(&mut rc.lcd_upload_ranges);
rc.renderer.upload_instance_ranges(&rc.terminal_instances, &cell_ranges)?;
rc.renderer.upload_lcd_ranges(&rc.terminal_lcd_instances, &lcd_ranges)?;
rc.renderer.upload_overlay_instances(&rc.overlay_instances)?;
rc.renderer.set_terminal_cell_count(rc.terminal_instance_count());
rc.renderer.set_overlay_count(rc.overlay_instances.len());
rc.renderer.upload_rect_instances(&rc.rect_instances);
```

Record the returned byte totals in `last_gpu_upload_bytes` and the frame
metrics. A frame with no terminal damage must issue no terminal or LCD range
writes while still rendering the persistent buffers.

- [ ] **Step 7: Implement explicit full-rebuild fallback**

Set a `full_rebuild` flag for resize, layout changes, font/theme changes,
atlas generation changes, device/surface reconfiguration, invalid slot
capacity, or invalid upload range. When set:

1. clear terminal layouts and persistent CPU storage;
2. mark all visible rows dirty;
3. rebuild all row slots;
4. merge one full range per terminal/LCD storage;
5. return the original error if the range still cannot fit.

Do not silently clamp a range or reduce the draw count to hide a missing row.

- [ ] **Step 8: Run GPU API checks**

Run:

```bash
cargo fmt --check
cargo test --lib merge_upload_ranges
cargo test --lib
cargo check --features profiling
cargo bench --bench build_instances -- --noplot --warm-up-time 1 --measurement-time 2 --sample-size 10
```

Expected: range tests pass, the renderer compiles with wgpu 29, and the
benchmark exposes lower work for one/small dirty ranges than full damage.

- [ ] **Step 9: Commit range uploads**

```bash
git add src/renderer/upload.rs src/renderer/mod.rs src/renderer/gpu.rs \
  src/app/renderer/mod.rs src/app/frame.rs
git commit -m "[PERF-ROI-01] feat: Upload only damaged terminal ranges." \
  -m "Keep terminal and LCD buffers persistent while drawing dynamic cursor and UI overlays separately." \
  -m "Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

### Task 6: Proving equivalence and fallback behavior

**Files:**
- Modify: `src/app/renderer/layout.rs`
- Modify: `src/app/renderer/damage.rs`
- Modify: `src/app/renderer/terminal.rs`
- Modify: `src/app/frame.rs`
- Modify: `src/app/mux/mod.rs`
- Modify: `benches/build_instances.rs`
- Test: unit tests in the listed modules

**Interfaces:**
- Produces a pure comparison helper for full and incremental row storage.
- Keeps `RenderContext::clear_all_row_caches` and atlas/font invalidation as full-rebuild entry points.
- Produces benchmark output for one dirty row, contiguous damage, scattered damage, and full-screen damage.

- [ ] **Step 1: Add a pure full-vs-incremental equivalence test**

Build a small synthetic 4-row terminal using `CellVertex` values with distinct
row positions. Build it once through the full path, update rows 1 and 3
through the slot path, and compare every visible terminal slot after applying
the same pane offset.

Assert that:

- clean rows retain their old vertices;
- dirty rows exactly match full rebuild vertices;
- transparent slot padding is identical;
- terminal and LCD storage have the same visible counts.

- [ ] **Step 2: Test all full-invalidation triggers**

Add tests that call the invalidation API for:

- terminal resize;
- pane origin/column change;
- font metric refresh;
- theme/color change;
- atlas generation change;
- missing row cache entry;
- row-slot capacity overflow;
- invalid GPU upload range.

Each test must assert `full_rebuild == true` and that the next build marks
every visible row dirty.

- [ ] **Step 3: Test dynamic overlay ordering**

Construct cursor, palette, and context-menu overlay instances with distinct
indices. Assert that the cursor is first, palette/context overlays follow,
and the terminal count is independent of the overlay count.

Add a blink-only test that changes only overlay slot zero and does not add
terminal upload ranges.

- [ ] **Step 4: Test PTY batch limits**

Feed a synthetic `PtyEventBatch` with more data than the configured work
budget. Assert that the first drain returns the bounded portion and leaves
`has_pending` true for the next iteration. Assert that exit, OSC-133, and
screen-clear events are not discarded when `DataReady` is coalesced.

- [ ] **Step 5: Expand the benchmark matrix**

In `benches/build_instances.rs`, add benchmark IDs:

- `incremental_one_dirty_row`;
- `incremental_eight_contiguous_rows`;
- `incremental_scattered_rows`;
- `incremental_full_damage`;
- `incremental_multi_pane_layout_change`.

Keep `build_frame_hit`, `build_frame_miss`, and the serial/parallel comparison
so the optimization cannot regress the existing baselines.

- [ ] **Step 6: Run the full focused validation**

Run:

```bash
cargo fmt --check
cargo test --lib
cargo bench --bench build_instances -- --noplot --warm-up-time 1 --measurement-time 2 --sample-size 10
cargo check --features profiling
```

Expected: all equivalence, invalidation, overlay, and PTY tests pass; the
incremental benchmarks show work proportional to dirty rows; the full-damage
case remains within the old full-frame order of magnitude.

- [ ] **Step 7: Commit correctness coverage**

```bash
git add src/app/renderer/layout.rs src/app/renderer/damage.rs \
  src/app/renderer/terminal.rs src/app/frame.rs src/app/mux/mod.rs \
  benches/build_instances.rs
git commit -m "[PERF-ROI-01] test: Cover incremental rendering equivalence." \
  -m "Verify dirty-row updates, invalidation triggers, overlay ordering, PTY batch limits, and full rebuild parity." \
  -m "Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

### Task 7: Measuring runtime impact and tuning thresholds

**Files:**
- Modify: `src/app/perf.rs`
- Modify: `src/app/mod.rs`
- Modify: `src/app/frame.rs`
- Modify: `src/app/renderer/overlay.rs`
- Modify: `benches/build_instances.rs`
- Documentation reference: `docs/superpowers/specs/2026-08-14-performance-roi-terminal-rendering-design.md`

**Interfaces:**
- Produces comparable counters for old full uploads and incremental uploads.
- Keeps the existing debug HUD usable for frame time, latency percentile, and upload KB.
- Produces a documented before/after measurement table for the approved scenarios.

- [ ] **Step 1: Add scenario labels to the debug metrics**

Record the current workload category in the metrics snapshot:

- `idle`;
- `interactive`;
- `pty_output`;
- `scroll`;
- `resize`;
- `multi_pane`.

Do not add a user-facing setting. Use internal labels in debug logs or the
existing F12 HUD only.

- [ ] **Step 2: Run the idle scenario**

Launch the release build with the default configuration and leave one terminal
idle for at least 30 seconds. Record:

- redraws and wakeups per second;
- p50/p95 frame time;
- p50/p95 input-to-pixel after a short typing sample;
- terminal/LCD upload bytes per unchanged frame.

Expected: unchanged frames issue no terminal/LCD range writes.

- [ ] **Step 3: Run the interactive scenario**

Type commands and paste a multiline command while recording input-to-pixel
latency. Repeat with one split pane and three tabs. Verify that PTY echo
coalescing does not delay visible input beyond the existing 4 ms coalescing
window or the 8 ms echo safety poll.

- [ ] **Step 4: Run the high-output scenario**

Use a controlled producer that writes at least 10,000 lines and then exits.
Record output throughput, frame time, CPU utilization, wakeups, dirty rows,
rebuilt rows, upload ranges, and upload bytes.

Compare against the pre-change benchmark/runtime baseline. Verify that input
remains responsive while output is active and that remaining PTY data is
processed on subsequent iterations rather than dropped.

- [ ] **Step 5: Run scroll, resize, and multi-pane scenarios**

Scroll through large scrollback, resize the window repeatedly, change font
scale if supported by the existing UI, and exercise split-pane movement.
Verify that each event triggers a full rebuild exactly when required and that
the next unchanged frame returns to partial/no terminal writes.

- [ ] **Step 6: Tune only measured thresholds**

Adjust only:

- PTY batch byte/time budget;
- redraw coalescing interval;
- row-slot growth threshold;
- full-rebuild compaction threshold.

For each adjustment, rerun the affected scenario and keep the value only when
it improves p95 latency or output throughput without increasing idle wakeups
or causing visible tearing/missing rows.

- [ ] **Step 7: Update the design record**

Add a short measurement table to the approved design document containing:

- machine/GPU and build profile;
- scenario;
- baseline p50/p95;
- incremental p50/p95;
- baseline upload bytes;
- incremental upload bytes;
- output throughput;
- observed regressions or limitations.

Do not claim a speedup without an equivalent baseline scenario.

- [ ] **Step 8: Run final repository checks**

Run:

```bash
cargo fmt --check
cargo test --lib
cargo check --features profiling
cargo bench --bench shaping -- --noplot --warm-up-time 1 --measurement-time 2 --sample-size 10
cargo bench --bench rasterize -- --noplot --warm-up-time 1 --measurement-time 2 --sample-size 10
cargo bench --bench search -- --noplot --warm-up-time 1 --measurement-time 2 --sample-size 10
cargo bench --bench build_instances -- --noplot --warm-up-time 1 --measurement-time 2 --sample-size 10
```

Expected: all library tests and the profiling build pass; existing shaping,
rasterization, search, and instance baselines remain within their measured
noise unless the runtime data documents a deliberate tradeoff.

- [ ] **Step 9: Commit the measured result**

```bash
git add src/app/perf.rs src/app/mod.rs src/app/frame.rs \
  src/app/renderer/overlay.rs benches/build_instances.rs \
  docs/superpowers/specs/2026-08-14-performance-roi-terminal-rendering-design.md
git commit -m "[PERF-ROI-01] chore: Record incremental rendering measurements." \
  -m "Capture runtime impact and threshold decisions for terminal latency and high-volume output." \
  -m "Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
```

## Verification Matrix

| Scenario | Primary metric | Required behavior |
|---|---|---|
| Idle, focused | wakeups/sec, terminal upload bytes | No unnecessary terminal/LCD writes. |
| Idle, unfocused/battery saver | wakeups/sec | Event loop remains parked except scheduled status updates. |
| Single-key echo | input-to-pixel p95 | No regression beyond current coalescing/safety windows. |
| Multiline paste | input-to-pixel p95, PTY batch count | Echo remains complete; no lost data. |
| 10,000-line output | throughput, CPU, p95 input latency | Output is faster or equal while input remains responsive. |
| Scrollback search/scroll | frame time, dirty rows | Only affected rows rebuild unless the operation requires full damage. |
| Resize/font/theme | full rebuild count | One explicit full rebuild, then incremental behavior resumes. |
| Atlas generation change | correctness, full rebuild count | No stale bind group or missing glyphs. |
| Multi-pane layout | upload bytes, visible output | Pane movement invalidates affected rows and preserves all other panes. |
