# GRAPH-ARCH-01 Chat Header LLM View Slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate the last remaining raw-`Config` LLM consumer — the chat panel header renderer — onto the existing `LlmRuntimeView`, closing out the LLM domain across all of `UiManager` and the renderer.

**Architecture:** `src/app/renderer/chat.rs::build_panel_header` currently branches on `config.llm.backend`/`config.llm.agent`/`config.llm.provider`/`config.llm.model` directly — the same four fields `LlmRuntimeView` (`src/config/llm_view.rs`) already exposes and that `UiManager::new`, `rewire_backend`, and `rewire_llm_provider` were migrated to in the prior two slices (commits `d9f6a66`..`f21b71d`, `64fc452`). No new struct or fields are needed; this is a pure call-site swap to `crate::config::llm_view::llm_runtime_view(config)`, matching the pattern already proven twice.

A domain survey (Term, Renderer/window/color usage) found no other candidate shaped like the LLM case — those reads are single-field, non-duplicated, and already routed through narrow existing methods (`ColorScheme::clear_color`, `apply_blur_translucency`). This slice is the only remaining high-value LLM-domain migration; it is not the start of a new "Renderer view" domain.

**Tech Stack:** Rust 2021, existing `LlmRuntimeView`/`llm_runtime_view` (already unit-tested for backend/agent/provider/model), `cargo test` workflow.

## Global Constraints

- No config schema changes.
- No behavior changes to the rendered header text (agent name / provider:model labels must be byte-identical to before).
- Only touch `src/app/renderer/chat.rs`; do not touch the `config.colors.*` reads in the same function — those are single-field reads unrelated to this slice.
- `build_panel_header`'s signature stays unchanged (private fn, single call site at `chat.rs:138`).

---

### Task 1: Migrate `build_panel_header` to `LlmRuntimeView`

**Files:**
- Modify: `src/app/renderer/chat.rs:437-474` (function `build_panel_header`)
- Test: none new — `llm_runtime_view` behavior is already covered by `src/config/llm_view.rs` unit tests (`llm_runtime_view_preserves_backend_agent_and_ui_width`, `llm_runtime_view_preserves_provider_defaults`, `llm_runtime_view_agent_path_requires_agent_config`); this task changes only the call site, no new logic.

**Interfaces:**
- Consumes: `crate::config::llm_view::llm_runtime_view(config: &Config) -> LlmRuntimeView` (existing, `src/config/llm_view.rs`) — fields used: `backend: LlmBackend`, `agent: Option<AcpAgentConfig>`, `provider_cfg: LlmConfig` (`.provider: String`, `.model: String`)
- Produces: no new interfaces — `build_panel_header` keeps its exact existing signature

- [ ] **Step 1: Confirm current code matches expectations before editing**

Run: `sed -n '436,475p' src/app/renderer/chat.rs`

Expected output includes this exact block (lines 451-474):

```rust
        let (left_label, center_full) = match &config.llm.backend {
            LlmBackend::Agent => {
                let agent = config.llm.agent.as_ref();
                let cmd = agent.map(|a| a.command.as_str()).unwrap_or("agent");
                let name = agent
                    .and_then(|a| a.display_name.as_deref())
                    .unwrap_or_else(|| {
                        std::path::Path::new(cmd)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or(cmd)
                    });
                (format!(" \u{25c8} {name}"), format!("agent:{name}"))
            }
            LlmBackend::Provider => {
                let provider = &config.llm.provider;
                let model = &config.llm.model;
                let short_model = short_chat_header_model_name(model);
                (
                    format!(" \u{2726} {short_model}"),
                    format!("{provider}:{model}"),
                )
            }
        };
```

If the file has drifted from this, stop and re-read the full function before proceeding — do not guess at the diff.

- [ ] **Step 2: Replace the raw `config.llm.*` reads with the view**

Replace the block from Step 1 with:

```rust
        let view = crate::config::llm_view::llm_runtime_view(config);
        let (left_label, center_full) = match &view.backend {
            LlmBackend::Agent => {
                let agent = view.agent.as_ref();
                let cmd = agent.map(|a| a.command.as_str()).unwrap_or("agent");
                let name = agent
                    .and_then(|a| a.display_name.as_deref())
                    .unwrap_or_else(|| {
                        std::path::Path::new(cmd)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or(cmd)
                    });
                (format!(" \u{25c8} {name}"), format!("agent:{name}"))
            }
            LlmBackend::Provider => {
                let provider = &view.provider_cfg.provider;
                let model = &view.provider_cfg.model;
                let short_model = short_chat_header_model_name(model);
                (
                    format!(" \u{2726} {short_model}"),
                    format!("{provider}:{model}"),
                )
            }
        };
```

Leave everything else in the function (the `config.colors.*` reads further down, the rest of the header layout code) untouched. `config` remains a used parameter because of those later reads, so no unused-variable warning is expected.

- [ ] **Step 3: Build to catch type errors**

Run: `cargo build`
Expected: PASS, no new warnings.

- [ ] **Step 4: Run the full test suite for regression**

Run: `cargo test`
Expected: PASS, same 98 tests as before this slice (no new tests added — this task has no new logic, only a call-site substitution of an already-tested accessor).

- [ ] **Step 5: Manual visual sanity check**

Run the app (`cargo run`), open the AI panel (`Leader a a`) with the default `llm.backend = "provider"` config and confirm the header still shows `✦ <model>`. If an agent is configured, switch `llm.backend = "agent"` and hot-reload, confirming the header shows `◈ <name>`. No visual difference from before this change is expected.

- [ ] **Step 6: Commit**

```bash
git add src/app/renderer/chat.rs
git commit -m "[TASK-2] refactor: Migrate chat panel header to LLM config view."
```

## Self-Review

- **Spec coverage:** The single remaining raw-Config LLM consumer (`build_panel_header`) is migrated; this closes the LLM domain across `UiManager` (prior two slices) and the renderer (this slice) — satisfying the stop condition ("most call sites consume domain views, not raw Config") for the LLM domain specifically.
- **Placeholder scan:** No TBD/TODO; every step has literal code.
- **Type consistency:** `LlmRuntimeView.provider_cfg: LlmConfig` (has `.provider: String`, `.model: String`) and `LlmRuntimeView.agent: Option<AcpAgentConfig>`, `LlmRuntimeView.backend: LlmBackend` all match the existing struct in `src/config/llm_view.rs` — no new fields required.
