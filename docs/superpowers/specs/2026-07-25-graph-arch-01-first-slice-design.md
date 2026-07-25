# GRAPH-ARCH-01 First Slice Design

## Goal

Reduce direct dependency on the broad `Config` object by introducing a narrow, read-only LLM configuration access boundary and migrating one high-impact consumer path.

## Scope

This slice implements only the first safe step of `GRAPH-ARCH-01`.

Included:

- Add a focused accessor module under `src/config/` for LLM-related reads.
- Expose only the fields needed by backend wiring and agent/provider selection.
- Migrate one consumer path (`UiManager` backend/rewire flow) to use the new accessor API.
- Add focused tests for accessor behavior and existing defaults.

Excluded:

- No `Config` schema restructuring yet.
- No multi-consumer migration in this slice.
- No behavior changes for default backend/provider resolution.

## Current Problem

`Config` is consumed broadly across unrelated subsystems. This increases coupling and makes later decomposition risky because many call sites depend on the full object shape.

## Proposed Design

### 1. Introduce a narrow LLM config view API

Create a small read-only API in `src/config/` that returns LLM-focused values through well-scoped getters. The API is derived from `Config` but does not expose unrelated configuration fields.

Design constraints:

- Read-only API.
- No cloning of large unrelated structures.
- Preserve existing defaults and fallback behavior.

### 2. Migrate one consumer path

Update the `UiManager` backend/rewire path to consume the new LLM accessor API rather than reading broad `Config` fields directly.

Why this consumer first:

- High impact on LLM behavior.
- Central enough to reduce coupling signal.
- Isolated enough to keep this change low risk.

### 3. Keep behavior unchanged

The migration is structural, not behavioral:

- Backend selection semantics remain identical.
- Agent/provider config interpretation remains identical.
- Existing config loading and runtime rewire behavior remain intact.

## Data Flow

1. `Config` is loaded as today.
2. Accessor module builds/returns LLM view data from `Config`.
3. `UiManager` wiring/rewire reads only via the LLM view API.
4. Runtime behavior remains unchanged.

## Error Handling

- No new silent fallbacks.
- Existing error paths are preserved.
- Accessor API should use existing typed config structures and defaults.

## Testing Strategy

- Unit tests for accessor outputs on representative configs:
  - provider backend
  - agent backend
  - defaulted values
- Regression coverage for the migrated wiring path to confirm unchanged behavior.

## Rollout and Follow-up

This is a stepping stone for subsequent `GRAPH-ARCH-01` slices:

1. Migrate additional LLM consumers to the accessor boundary.
2. Repeat with other domains (`ui`, `term`, `renderer`) via domain-specific views.
3. Only then split `Config` internals into bounded sub-config aggregates.

## Success Criteria

- One central consumer path no longer depends on broad `Config` access.
- No behavior changes in backend/provider/agent wiring.
- Tests confirm parity with prior behavior.
