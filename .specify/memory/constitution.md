<!--
Sync Impact Report
- Version change: (template, unversioned) -> 1.0.0
- Modified principles: none (initial ratification)
- Added sections: Core Principles I-VII, Platform & Technical Constraints,
  Development Workflow & Quality Gates, Governance
- Removed sections: none
- Templates:
  - .specify/templates/plan-template.md: OK (Constitution Check is generic; gates are
    derived from this file at plan time, no edit needed)
  - .specify/templates/spec-template.md: OK (no mandatory sections added/removed)
  - .specify/templates/tasks-template.md: OK (no new task categories required)
  - .claude/skills/speckit-*/SKILL.md: OK (no agent-specific references requiring change)
  - AGENTS.md / CLAUDE.md: OK (runtime guidance; consistent with this file)
- Deferred TODOs: none
-->

# PetruTerm Constitution

## Core Principles

### I. Preserve the Two-Binary, One-Core Architecture

The crate ships two binaries (`petruterm` wgpu/winit, `gpui-petruterm` gpui) that MUST share
one terminal/config/LLM core. Chrome-specific code lives only in `src/app`, `src/ui`,
`src/renderer` (wgpu) or `src/gpui_shell` (gpui). Logic needed by both binaries MUST live in
the shared modules (`src/term`, `src/config`, `src/llm`, `src/font`, `src/platform`, shared
`src/ui` state) and MUST NOT be duplicated per binary. Neither binary may be removed, merged
into the other, or made to depend on the other's chrome code without an amendment.
Rationale: feature parity and a single source of truth for behavior.

### II. Delegate, Do Not Reimplement, the Terminal Engine

`alacritty_terminal` owns the grid, VTE parsing, and PTY. Code MUST NOT reimplement grid,
escape-sequence, or PTY logic. Renderers read cells from the grid and map them to output.
Rationale: correctness and maintenance cost of terminal emulation.

### III. Config Is Lua, Fully Re-evaluated, Atomically Swapped

Configuration is a Lua 5.4 DSL (mlua) deserialized into plain `serde::Deserialize` structs.
On change, Lua is re-evaluated in full and the whole `Config` is swapped, then dependent
state (keybinds, palette, metrics) is rebuilt. Partial in-place mutation of live config is
prohibited. Default config files in `config/default/` MUST stay in sync with any new option.
A config view/wrapper type MUST exist only when 2+ files reimplement the same non-trivial
logic; single-field pass-throughs are not allowed.

### IV. Threading and Async Discipline

GPU/window work stays on the main thread. All I/O-bound work (LLM, ACP, file watching,
remote relay) MUST use `async`/`await` on tokio; blocking calls on the render or UI thread
are prohibited. Hot render paths MUST NOT allocate or leak per frame; known render/PTY
invariants (space cells, scrollback offset, exit event, PTY env, ligatures, font index)
MUST NOT be altered without a regression test or documented verification.

### V. LLM Features Are Optional and Provider-Agnostic

All LLM functionality MUST be disableable via `config.llm.enabled = false` with no impact on
terminal operation. Providers (OpenRouter, Ollama, LMStudio, Copilot) and the ACP backend
stay behind the existing `src/llm` abstractions. API keys MUST resolve in the documented order
(Lua config, env var, macOS Keychain) and MUST NOT be logged or committed.

### VI. Stable Dependencies Only

Dependencies MUST be the latest stable published release. Pinning prerelease versions or
git tips is prohibited. New dependencies require justification in the plan (Complexity
Tracking) and MUST pass `cargo audit` in CI.

### VII. Simplicity and Module Size

Prefer the simplest working change; no speculative features or single-use abstractions.
Module files MUST stay under 400 lines and be split when exceeded. Errors use `anyhow::Result`
for application code and `thiserror` for library-style errors.

## Platform & Technical Constraints

- Target is macOS only; building either binary requires full Xcode.app (Metal shader compiler).
  Cross-platform abstractions MUST NOT be introduced speculatively.
- Plugin system (former Phase 4) is cancelled; `src/plugins/` MUST NOT be reintroduced without
  an amendment.
- Keybind semantics are preserved: leader `Ctrl+F` (1000ms), `tmux` default style; `gpui-petruterm`
  always uses tmux-style binds. Existing bindings MUST NOT change meaning without a spec.
- Public config keys, `config/default/` layout, and persisted workspace JSON format MUST remain
  backward compatible, or ship with a migration.

## Development Workflow & Quality Gates

- Read a file before editing; grep all callers before modifying a function.
- Before merge, `./scripts/ci-local.sh` MUST pass (check, test, clippy, fmt, audit under
  `RUSTFLAGS=-D warnings`).
- Commits follow `[TASK_ID] type: Message.` with type in `feat|fix|chore|refactor`.
- Architecture changes update `.context/architecture/SYSTEM_MAP.md`; session handoff goes to
  `.context/core/SESSION_STATE.md`; debt goes to `.context/quality/TECHNICAL_DEBT.md`.
- Specs and plans MUST include a Constitution Check against Principles I-VII.

## Governance

This constitution supersedes other practices. Amendments require a documented change to this
file, a Sync Impact Report, and a version bump. Versioning is semantic: MAJOR for removing or
redefining a principle, MINOR for adding a principle or materially expanding guidance, PATCH
for clarifications. Violations in a plan MUST be justified in Complexity Tracking; unjustified
violations block merge. Reviews MUST verify compliance. Runtime guidance lives in `AGENTS.md`.

**Version**: 1.0.0 | **Ratified**: 2026-11-06 | **Last Amended**: 2026-11-06
