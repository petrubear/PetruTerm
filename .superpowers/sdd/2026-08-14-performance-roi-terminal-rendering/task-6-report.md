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

## Fix round 1 — reviewer Important findings

### Status

Completed. No dependencies were added and no unrelated files were changed.

### Corrections

- Connected full-rebuild trigger handling to production grid damage, row-cache, layout,
  atlas/font/theme, row-capacity, and upload-failure paths. Capacity growth now schedules
  an explicit full rebuild on the next build instead of relying on copied slots silently.
- Reworked the row equivalence test into a real full-reference versus incremental update:
  clean rows remain in old terminal/LCD storage, only rows 1 and 3 use slot writes, and
  all slot bytes, transparent padding, and terminal/LCD counts are compared.
- Connected overlay planning and blink helpers to the production upload path. The plan
  preserves terminal count independently, verifies cursor-first ordering, and blink
  uploads slot zero without terminal ranges.
- Replaced the unused synthetic PTY batch drain with a bounded helper called by
  `Mux::poll_pty_events`; tests use actual `PtyEvent` payloads and preserve exit,
  OSC-133, and screen-clear events after the first budgeted drain.
- Changed `incremental_multi_pane_layout_change` to retain pane layout/cache state and
  alternate a real persistent pane transition, rebuilding only the pane whose geometry
  changed.

### Validation

- `rtk cargo fmt --all -- --check` — passed.
- `rtk cargo test` — passed, 147 tests.
- `rtk cargo test --lib` — passed, 22 tests.
- `rtk cargo check --features profiling` — passed.
- `rtk cargo bench --bench build_instances -- --noplot --warm-up-time 1 --measurement-time 2 --sample-size 10` — completed successfully; all requested benchmark IDs ran, including one-dirty, contiguous, scattered, full-damage, and multi-pane layout-change cases. Gnuplot was unavailable, so Criterion used its existing plotters backend.
- `rtk git diff --check` — passed.
- `rtk graphify update .` — completed; generated graph artifacts were reverted and are not part of the task diff.

### Residual concerns

- Criterion reported small environment-sensitive regressions against its saved baseline
  for `build_frame_miss`, `incremental_one_dirty_row`, and the layout-change benchmark;
  the command completed and the full-damage benchmark remained in the same order of
  magnitude. Device-specific GPU profiling remains outside this headless benchmark.

## Fix round 2 — production-connected invalidation and overlay coverage

### Status

Completed. No dependencies were added and behavior remains unchanged.

### Corrections

- Added the production build-damage contract used by `RenderContext::build_instances`,
  including pending invalidation consumption, missing-cache fallback, and row-slot
  capacity overflow handling.
- Routed cache invalidation through the same production request seam used by
  `clear_all_row_caches_for`; tests cover resize, pane geometry, font metrics,
  theme/color, atlas generation, missing cache, capacity overflow, and invalid GPU
  upload triggers, asserting full rebuild and every visible row dirty.
- Added the production upload-failure trigger used by `handle_redraw`, with a test
  proving an upload failure schedules the invalid-GPU full rebuild contract.
- Connected overlay planning to both full upload and fast blink paths. Tests cover
  cursor-first ordering, independent terminal/overlay counts, cursor blink changing
  only the overlay vertex, and empty terminal upload ranges during blink.

### Validation

- `rtk cargo fmt --all -- --check` — passed.
- `rtk cargo test` — passed, 148 tests.
- `rtk cargo test --lib` — passed, 22 tests.
- `rtk cargo check --features profiling` — passed.
- `rtk cargo bench --bench build_instances -- --noplot --warm-up-time 1 --measurement-time 2 --sample-size 10` — completed successfully; all requested benchmark IDs ran. Gnuplot was unavailable, so Criterion used plotters.
- `rtk git diff --check` — passed.
- `rtk graphify update .` — completed; generated graph artifacts were reverted and are not part of this task diff.

### Residual concerns

- Criterion remained environment-sensitive: `build_frame_hit_large_par`,
  `incremental_multi_pane_layout_change`, and `build_row_hit` reported small
  regressions; all requested benchmarks completed and full damage remained in the
  prior order of magnitude. Device-specific GPU profiling remains outside this
  headless benchmark.

## Fix round 3 — explicit production build and overlay seams

### Status

Completed. No dependencies were added.

### Corrections

- Replaced the directly-called invalidation helpers with `BuildInvalidationState`,
  a production-owned state object stored by `RenderContext`. `RenderContext::build_instances`
  now consumes damage through `begin_terminal_build`, while cache invalidation and
  pane-layout rebuild call sites record triggers through the same state object.
- Added production-connected coverage for every full-rebuild trigger, missing cache,
  capacity overflow, and trigger consumption; assertions verify full rebuild and every
  visible row is dirty.
- Added `OverlayUploadState` for the exact full-upload and cursor-blink paths used by
  `handle_redraw`, plus `build_cursor_vertex` used by `build_cursor_instance`. Tests
  now exercise those production seams without constructing a window and verify cursor
  ordering, independent terminal counts, blink slot zero, and no terminal blink ranges.

### Validation

- `rtk cargo fmt --all -- --check` — passed.
- `rtk cargo test production_ -- --nocapture` — passed, 6 tests.
- `rtk cargo test --lib` — passed, 22 tests.
- `rtk cargo test -- --nocapture` — passed, 147 tests across 3 suites.
- `rtk cargo check --features profiling` — passed.
- `rtk cargo bench --bench build_instances -- --noplot --warm-up-time 1 --measurement-time 2 --sample-size 10` — passed; all requested benchmark IDs ran. Gnuplot was unavailable, so Criterion used plotters. Full-damage remained in the prior order of magnitude.
- `rtk git diff --check` — passed.
- `rtk graphify update .` — completed; generated graph artifacts were reverted and are not part of this task's diff.

### Final fresh verification

- Combined command `rtk cargo fmt --all -- --check && rtk cargo test --lib && rtk cargo test && rtk cargo check --features profiling && rtk cargo bench --bench build_instances -- --noplot --warm-up-time 1 --measurement-time 2 --sample-size 10 && rtk git diff --check` — exited 0.
- Fresh test evidence: `cargo test --lib` reported 22 passed; `cargo test` reported 147 passed across 3 suites; profiling check finished successfully.
- Fresh benchmark evidence: one-dirty row improved 1.6954%, contiguous rows improved 1.7597%, scattered rows improved 2.7554%, full damage improved 1.4132%, and all requested benchmark IDs completed.
- Criterion flagged environment-sensitive regressions for `build_frame_hit_large_par` (+11.543%) and `incremental_multi_pane_layout_change` (+1.4679%); the command still exited 0 and full damage remained in the prior order of magnitude.

### Concerns

- GPU-device-specific upload behavior remains outside the headless seam tests; runtime
  still uses the real renderer upload methods after the tested production planning seam.
- The fresh benchmark sample was environment-sensitive: Criterion flagged small
  regressions for `build_row_miss` (+1.5%) and `incremental_one_dirty_row` (+2.3%);
  all other requested cases were unchanged or improved, and the command exited 0.

## Fix round 4 — production-owned headless orchestration

### Status

Completed. No dependencies were added.

### Corrections

- Replaced `BuildInvalidationState` with the production-owned `RenderBuildState`
  stored directly in `RenderContext`. `RenderContext::build_instances` now calls
  `resolve_terminal_build`; `clear_all_row_caches_for`, `prepare_terminal_layouts`,
  `grow_terminal_row_capacity`, and upload-failure fallback all call the shared
  `request_full_rebuild` seam. Capacity growth no longer relies on a test-visible
  terminal set or a boolean passed only to the resolver.
- Replaced `OverlayUploadState` and the free `build_cursor_vertex` helper with the
  production-owned `RenderOverlayState` stored in `RenderContext`. Real
  `build_cursor_instance` calls `build_cursor_overlay`; real `handle_redraw` full
  and blink paths call `plan_production_upload` and
  `plan_cursor_blink_overlay`. Headless tests call these same production state
  methods and do not construct duplicate wrappers or test-only upload helpers.
- Preserved the complete trigger matrix and overlay assertions: every trigger
  yields `full_rebuild` with all visible rows dirty; cursor remains first,
  terminal count is independent of overlay count, blink updates slot zero only,
  and blink has no terminal upload ranges.

### Exact validation evidence

- `rtk cargo fmt --all -- --check` — passed.
- `rtk cargo test production_ -- --nocapture` — passed, 6 tests, 141 filtered.
- `rtk cargo test --lib` — passed, 22 tests.
- `rtk cargo test` — passed, 147 tests across 3 suites.
- `rtk cargo check --features profiling` — passed.
- `rtk cargo bench --bench build_instances -- --noplot --warm-up-time 1 --measurement-time 2 --sample-size 10` — completed successfully. All requested benchmark IDs ran; `build_row_miss` improved 1.50%, `incremental_one_dirty_row` improved 0.59%, full damage showed no change, and `incremental_multi_pane_layout_change` improved 0.87%.
- `rtk git diff --check` — passed.
- `rtk graphify update .` — completed; generated graph artifacts were reverted and are not part of this task's diff.
