# GRAPH-ARCH-01 First Slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce a narrow LLM config access boundary and migrate one `UiManager` backend wiring path to reduce direct coupling to the broad `Config` object.

**Architecture:** Add a small `config::llm_view` module that derives a read-only `LlmRuntimeView` from `Config`. Keep `Config` as the source-of-truth and preserve behavior by migrating `UiManager::rewire_backend` and `UiManager::rewire_llm_provider` internals to consume the view while leaving external signatures stable.

**Tech Stack:** Rust 2021, serde config structs, winit event loop, tokio runtime, existing `cargo test` unit-test workflow.

## Global Constraints

- No config schema changes in this slice.
- No behavior changes for backend/provider/agent wiring.
- Keep changes incremental: migrate one consumer path (`UiManager` rewire flow) only.
- Preserve existing defaults and trust-gated local loading behavior.
- Keep module files focused and aligned with existing project patterns.

---

### Task 1: Add a narrow LLM runtime view module

**Files:**
- Create: `src/config/llm_view.rs`
- Modify: `src/config/mod.rs`
- Test: `src/config/llm_view.rs` (unit tests in `#[cfg(test)]`)

**Interfaces:**
- Consumes: `crate::config::schema::{Config, LlmBackend, AcpAgentConfig, LlmConfig}`
- Produces:
  - `pub struct LlmRuntimeView { pub enabled: bool, pub backend: LlmBackend, pub panel_width_cols: u16, pub agent: Option<AcpAgentConfig>, pub provider_cfg: LlmConfig }`
  - `pub fn llm_runtime_view(config: &Config) -> LlmRuntimeView`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn llm_runtime_view_preserves_backend_agent_and_ui_width() {
    let mut config = Config::default();
    config.llm.enabled = true;
    config.llm.backend = LlmBackend::Agent;
    config.llm.ui.width_cols = 72;
    config.llm.agent = Some(AcpAgentConfig {
        command: "npx".into(),
        args: vec!["-y".into(), "@agentclientprotocol/claude-agent-acp".into()],
        env: vec![("FOO".into(), "bar".into())],
        display_name: Some("Claude".into()),
    });

    let view = llm_runtime_view(&config);
    assert!(view.enabled);
    assert_eq!(view.backend, LlmBackend::Agent);
    assert_eq!(view.panel_width_cols, 72);
    assert_eq!(view.agent.as_ref().unwrap().command, "npx");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test llm_runtime_view_preserves_backend_agent_and_ui_width --lib`  
Expected: FAIL (`llm_runtime_view` / `LlmRuntimeView` missing)

- [ ] **Step 3: Write minimal implementation**

```rust
#[derive(Debug, Clone)]
pub struct LlmRuntimeView {
    pub enabled: bool,
    pub backend: LlmBackend,
    pub panel_width_cols: u16,
    pub agent: Option<AcpAgentConfig>,
    pub provider_cfg: LlmConfig,
}

pub fn llm_runtime_view(config: &Config) -> LlmRuntimeView {
    LlmRuntimeView {
        enabled: config.llm.enabled,
        backend: config.llm.backend.clone(),
        panel_width_cols: config.llm.ui.width_cols,
        agent: config.llm.agent.clone(),
        provider_cfg: config.llm.clone(),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test llm_runtime_view_preserves_backend_agent_and_ui_width --lib`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/config/llm_view.rs src/config/mod.rs
git commit -m "feat: Add llm runtime config view"
```

---

### Task 2: Add regression tests for provider-oriented defaults in the view

**Files:**
- Modify: `src/config/llm_view.rs`
- Test: `src/config/llm_view.rs`

**Interfaces:**
- Consumes: `llm_runtime_view(config: &Config) -> LlmRuntimeView`
- Produces:
  - Verified behavior contract for defaults (`backend=Provider`, default model/provider, width passthrough)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn llm_runtime_view_preserves_provider_defaults() {
    let config = Config::default();
    let view = llm_runtime_view(&config);
    assert_eq!(view.backend, LlmBackend::Provider);
    assert_eq!(view.provider_cfg.provider, "openrouter");
    assert_eq!(view.provider_cfg.model, "meta-llama/llama-3.1-8b-instruct:free");
    assert_eq!(view.panel_width_cols, config.llm.ui.width_cols);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test llm_runtime_view_preserves_provider_defaults --lib`  
Expected: FAIL until the view carries provider config values correctly

- [ ] **Step 3: Write minimal implementation update**

```rust
// Ensure `provider_cfg` is copied from `config.llm` without dropping fields.
provider_cfg: config.llm.clone(),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test llm_runtime_view_preserves_provider_defaults --lib`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/config/llm_view.rs
git commit -m "test: Lock llm runtime view defaults"
```

---

### Task 3: Migrate `UiManager` rewire flow to consume `LlmRuntimeView`

**Files:**
- Modify: `src/app/ui/providers.rs`
- Modify: `src/app/ui/mod.rs` (if needed for call-site consistency only)
- Test: `src/config/llm_view.rs` and existing UI-related lib tests

**Interfaces:**
- Consumes:
  - `llm_runtime_view(config: &Config) -> LlmRuntimeView`
  - `UiManager::rewire_backend(&mut self, config: &Config, wakeup_proxy: EventLoopProxy<()>)`
  - `UiManager::rewire_llm_provider(&mut self, config: &Config)`
- Produces:
  - `rewire_backend` internals branch on `view.backend`
  - `rewire_llm_provider` uses `view.provider_cfg` and `view.panel_width_cols`
  - External method signatures unchanged

- [ ] **Step 1: Write a failing test for rewire branching behavior surface**

```rust
#[test]
fn llm_runtime_view_agent_path_requires_agent_config() {
    let mut config = Config::default();
    config.llm.enabled = true;
    config.llm.backend = LlmBackend::Agent;
    config.llm.agent = None;
    let view = llm_runtime_view(&config);
    assert!(matches!(view.backend, LlmBackend::Agent));
    assert!(view.agent.is_none());
}
```

- [ ] **Step 2: Run test to verify it fails (if not already covered)**

Run: `cargo test llm_runtime_view_agent_path_requires_agent_config --lib`  
Expected: FAIL until view and branch assumptions are explicit

- [ ] **Step 3: Implement minimal migration in `providers.rs`**

```rust
let view = crate::config::llm_view::llm_runtime_view(config);
match view.backend {
    LlmBackend::Provider => {
        self.acp_session = None;
        self.rewire_llm_provider(config);
    }
    LlmBackend::Agent => {
        self.panel_width_cols = view.panel_width_cols;
        if let Some(agent_cfg) = view.agent {
            self.acp_pending_connect = Some(super::spawn_acp_connect(
                &self.tokio_rt,
                agent_cfg,
                cwd,
                wakeup_proxy,
            ));
        } else {
            self.llm_init_error =
                Some("llm.agent config is required when backend = \"agent\"".into());
        }
    }
}
```

- [ ] **Step 4: Run focused verification**

Run: `cargo test llm_runtime_view_ --lib`  
Expected: PASS for all `llm_view` tests

Run: `cargo test --lib`  
Expected: PASS (no regression in wiring paths)

- [ ] **Step 5: Commit**

```bash
git add src/app/ui/providers.rs src/app/ui/mod.rs src/config/llm_view.rs src/config/mod.rs
git commit -m "refactor: Rewire UiManager through llm config view"
```

---

### Task 4: Final consistency pass and docs alignment

**Files:**
- Modify: `.context/quality/TECHNICAL_DEBT.md` (update note under `GRAPH-ARCH-01` next slice progress)
- Modify: `.context/core/ACTIVE_CONTEXT.md` (mark P1.1 slice progress if your workflow requires it)
- Test: none (documentation-only)

**Interfaces:**
- Consumes: Completed code changes from Tasks 1–3
- Produces: Updated context notes showing P1.1 first slice completed

- [ ] **Step 1: Update debt progress note**

```md
GRAPH-ARCH-01 progress: first slice done — added LLM runtime config view and migrated UiManager rewire flow.
```

- [ ] **Step 2: Review working tree for unrelated edits**

Run: `git --no-pager status --short`  
Expected: only intended files staged for this slice

- [ ] **Step 3: Run final targeted verification**

Run: `cargo test llm_runtime_view_ --lib`  
Expected: PASS

- [ ] **Step 4: Commit docs/context updates**

```bash
git add .context/quality/TECHNICAL_DEBT.md .context/core/ACTIVE_CONTEXT.md
git commit -m "chore: Record GRAPH-ARCH-01 first-slice progress"
```

## Self-Review

- **Spec coverage:** This plan covers the approved first slice: new LLM accessor boundary, one consumer migration, behavior parity, and focused tests.
- **Placeholder scan:** No `TODO`/`TBD` placeholders; each task includes concrete file paths, commands, and code snippets.
- **Type consistency:** `LlmRuntimeView` and `llm_runtime_view` signatures are consistent across all tasks and call sites.
