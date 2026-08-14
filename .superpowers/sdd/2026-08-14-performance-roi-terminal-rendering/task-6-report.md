# Task 6 — Incremental/Full Rendering Equivalence Report

## Status
Implementation complete. No dependencies were added.

## Implementation details

- Added pure, slot-based storage equivalence coverage for full and incremental row writes. Comparison includes each row slot's complete byte representation, including transparent padding, and rejects length or bounds mismatches.
- Added deterministic full-rebuild fallback coverage for missing row-cache entries and a trigger matrix covering resize, pane geometry, font metrics, theme, atlas generation, missing cache, slot capacity overflow, and invalid GPU ranges.
- Preserved safe fallback behavior: missing cache state marks every visible row dirty rather than silently omitting rows; row-capacity growth invalidates terminal layout for the next frame.
- Added overlay ordering and blink coverage. Cursor is emitted before palette/context overlays; blink state changes affect overlay visibility/upload planning without adding terminal upload ranges.
- Added bounded PTY draining (256 events per poll), pending-work propagation, redraw scheduling while work remains, and tests confirming data batches plus exit/special events are preserved across the limit.
- Explicitly clears row caches on theme changes so theme invalidation cannot reuse stale terminal instances.
- Extended the build benchmark matrix with one-row, contiguous, scattered, full-damage, and multi-pane layout-change scenarios.

## Files changed

- `src/app/renderer/layout.rs`
- `src/app/renderer/damage.rs`
- `src/app/renderer/terminal.rs`
- `src/app/frame.rs`
- `src/app/mux/mod.rs`
- `src/app/mod.rs`
- `src/app/ui/mod.rs`
- `benches/build_instances.rs`

## TDD RED/GREEN evidence

Focused tests were added for the new observable behaviors and run while iterating. The missing-cache fallback, row-slot equivalence, full-rebuild trigger matrix, overlay order, PTY batch preservation, and PTY work-limit cases were used as red/green targets; the focused test runs passed after the minimal production changes. Existing tests remained green throughout final validation.

## Validation

- `cargo fmt --check` — passed.
- `cargo test` — passed, 146 tests.
- `cargo test --lib` — passed, 22 tests.
- `cargo check --features profiling` — passed.
- Criterion benchmark matrix command — passed; requested benchmark IDs ran, including `build_frame_dirty_rows/incremental_one_dirty_row`, `incremental_eight_contiguous_rows`, `incremental_scattered_rows`, `incremental_full_damage`, and `incremental_multi_pane_layout_change`.
- `git diff --check` — passed.
- `graphify update .` was run after changes; generated graph artifacts were reverted and are not part of this task's diff.

## Self-review

The implementation keeps existing persistent row-slot, dirty-row revision, merged GPU range, overlay, and PTY interfaces intact. Bounds are checked before slot comparisons/writes, full-frame fallback remains explicit, and pending PTY work is surfaced to the event loop instead of being dropped. Changes are limited to Task 6 behavior and its deterministic tests/benchmarks.

## Concerns

- The benchmark suite requires the existing headless wgpu/font environment and is not a substitute for device-specific profiling.
- The full-rebuild trigger matrix validates the shared fallback contract; individual production call sites still depend on their existing invalidation paths.

## Commit

`[PERF-ROI-01] test: Prove incremental rendering equivalence` (final HEAD)
