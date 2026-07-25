# GRAPH-ARCH-01 LLM Domain Closure Slice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish migrating every remaining raw-`Config` *read* of LLM fields onto `LlmRuntimeView`, and collapse the agent-display-name derivation (currently duplicated in two files) into one shared helper.

**Architecture:** The prior slice (commits `d9f6a66`..`44a11d5`) migrated `UiManager::new`, `rewire_backend`, `rewire_llm_provider`, and `build_panel_header` onto `LlmRuntimeView` (`src/config/llm_view.rs`), and its own final review flagged that the claim "LLM domain closed" was premature: `src/app/ui/providers.rs::handle_slash_command`'s `"model"` and `"agent"` slash commands still read `config.llm.backend`/`config.llm.provider`/`config.llm.model`/`config.llm.agent` raw, and duplicate — inline, a second time — the exact agent-name-derivation logic (`display_name`, else basename of `command`) that `build_panel_header` also has. This slice: (1) extracts that derivation into `pub fn agent_display_name(agent: Option<&AcpAgentConfig>) -> Option<&str>` next to `LlmRuntimeView`, unit-tested on its own; (2) migrates `providers.rs`'s two slash-command arms to use the view + helper for reads, while leaving the arms' *writes* (`config.llm.model = ...`, `config.llm.agent = ...`) on raw `Config`, since `Config` stays the mutation source-of-truth and a read-only view cannot replace a write; (3) updates `build_panel_header` to call the new shared helper instead of its own inline copy, actually removing the duplication instead of just moving it.

**Verified scope (run before writing this plan, re-run in Task 2's Step 1 to confirm no drift):**

```bash
grep -rn "config\.llm\." src/ --include=*.rs | grep -v "^src/config/"
```

As of commit `44a11d5` this returns exactly 7 lines, all in `src/app/ui/providers.rs`: line 212 (read, `backend`), line 223 (read, `provider`+`model`), line 225 (**write**, `model =`), line 238 (read, `backend`), lines 244-256 (read, `agent`, spans multiple lines so a single-line grep only catches part of it), line 263 (**write**, `agent.as_mut()`), line 266 (**write**, `agent = Some(...)`). Every read in that list is migrated by Task 2 below; every write is intentionally left alone. After this slice, no raw `config.llm.*` **read** exists anywhere outside `src/config/`.

**Tech Stack:** Rust 2021, existing `LlmRuntimeView`/`llm_runtime_view` (`src/config/llm_view.rs`), `cargo test`.

## Global Constraints

- No config schema changes.
- No behavior changes anywhere — every migrated read must produce byte-identical output to what it replaces. Each task below includes the equivalence reasoning; verify it, don't just trust it.
- Writes to `config.llm.*` (`providers.rs:225,263,266`) stay on raw `&mut Config` — do not attempt to route mutations through the read-only view.
- `handle_slash_command`'s and `build_panel_header`'s signatures stay unchanged.
- Do not touch `config.colors.*` reads anywhere — out of scope, unrelated concern (confirmed thin/non-duplicated in the prior slice's domain survey).

---

### Task 1: Add `agent_display_name` helper to the LLM view module

**Files:**
- Modify: `src/config/llm_view.rs`

**Interfaces:**
- Consumes: `crate::config::schema::AcpAgentConfig` (existing, fields: `command: String`, `args: Vec<String>`, `env: Vec<(String, String)>`, `display_name: Option<String>`)
- Produces: `pub fn agent_display_name(agent: Option<&AcpAgentConfig>) -> Option<&str>` — `None` iff `agent` is `None`; otherwise `Some(agent.display_name)` if set, else `Some(basename of agent.command)`.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `src/config/llm_view.rs`:

```rust
    #[test]
    fn agent_display_name_prefers_display_name_over_command_basename() {
        let agent = AcpAgentConfig {
            command: "/usr/local/bin/claude-agent-acp".into(),
            args: vec![],
            env: vec![],
            display_name: Some("Claude".into()),
        };
        assert_eq!(agent_display_name(Some(&agent)), Some("Claude"));
    }

    #[test]
    fn agent_display_name_falls_back_to_command_basename() {
        let agent = AcpAgentConfig {
            command: "/usr/local/bin/claude-agent-acp".into(),
            args: vec![],
            env: vec![],
            display_name: None,
        };
        assert_eq!(agent_display_name(Some(&agent)), Some("claude-agent-acp"));
    }

    #[test]
    fn agent_display_name_none_when_no_agent() {
        assert_eq!(agent_display_name(None), None);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test agent_display_name --lib`
Expected: FAIL (`agent_display_name` not found)

- [ ] **Step 3: Write the minimal implementation**

Add above or below `llm_runtime_view` in `src/config/llm_view.rs` (outside the test module):

```rust
pub fn agent_display_name(agent: Option<&AcpAgentConfig>) -> Option<&str> {
    let agent = agent?;
    Some(agent.display_name.as_deref().unwrap_or_else(|| {
        std::path::Path::new(&agent.command)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&agent.command)
    }))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test agent_display_name --lib`
Expected: PASS (3/3)

Run: `cargo test --lib`
Expected: PASS, all prior `llm_view` tests still pass (no regression to `llm_runtime_view`).

- [ ] **Step 5: Commit**

```bash
git add src/config/llm_view.rs
git commit -m "feat: Add agent_display_name helper to LLM config view."
```

---

### Task 2: Migrate `handle_slash_command`'s `"model"` and `"agent"` arms to the view

**Files:**
- Modify: `src/app/ui/providers.rs` (function `handle_slash_command`, the `"model"` arm at lines ~211-236 and the `"agent"` arm at lines ~237-284 as of commit `44a11d5` — re-run the grep from this plan's header first; if line numbers or code text have drifted, stop and re-read the function before editing)

**Interfaces:**
- Consumes: `crate::config::llm_view::llm_runtime_view(config: &Config) -> LlmRuntimeView` and `crate::config::llm_view::agent_display_name(Option<&AcpAgentConfig>) -> Option<&str>` (from Task 1)
- Produces: no new interfaces — `handle_slash_command`'s signature is unchanged

- [ ] **Step 1: Confirm current code matches expectations**

Run: `sed -n '211,236p' src/app/ui/providers.rs` and `sed -n '237,284p' src/app/ui/providers.rs`

Expected: the `"model"` arm matches:

```rust
            "model" => {
                let msg = match &config.llm.backend {
                    LlmBackend::Agent => {
                        if args.is_empty() {
                            "Agent mode: use /agent to switch agents.".to_string()
                        } else {
                            "Cannot set model in agent mode. Switch to provider backend first."
                                .to_string()
                        }
                    }
                    LlmBackend::Provider => {
                        if args.is_empty() {
                            format!("Active: {}:{}", config.llm.provider, config.llm.model)
                        } else {
                            config.llm.model = args.to_string();
                            self.rewire_backend(config, wakeup_proxy.clone());
                            format!("Model set to '{args}'.")
                        }
                    }
                };
                self.panel_mut()
                    .messages
                    .push(crate::llm::ChatMessage::assistant(msg));
                self.panel_mut().dirty = true;
                true
            }
```

and the `"agent"` arm matches:

```rust
            "agent" => {
                let msg = match &config.llm.backend {
                    LlmBackend::Provider => {
                        "Provider mode active. Use /model to change models.".to_string()
                    }
                    LlmBackend::Agent => {
                        if args.is_empty() {
                            let name = config
                                .llm
                                .agent
                                .as_ref()
                                .and_then(|a| a.display_name.as_deref())
                                .or_else(|| {
                                    config.llm.agent.as_ref().map(|a| {
                                        std::path::Path::new(&a.command)
                                            .file_name()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or(&a.command)
                                    })
                                });
                            match name {
                                Some(n) => format!("Active agent: {n}"),
                                None => "No agent configured. Set llm.agent.command in config."
                                    .to_string(),
                            }
                        } else {
                            if let Some(agent_cfg) = config.llm.agent.as_mut() {
                                agent_cfg.command = args.to_string();
                            } else {
                                config.llm.agent = Some(crate::config::schema::AcpAgentConfig {
                                    command: args.to_string(),
                                    args: vec![],
                                    env: vec![],
                                    display_name: None,
                                });
                            }
                            self.acp_session = None;
                            self.rewire_backend(config, wakeup_proxy.clone());
                            format!("Agent set to '{args}'. Reconnecting...")
                        }
                    }
                };
                self.panel_mut()
                    .messages
                    .push(crate::llm::ChatMessage::assistant(msg));
                self.panel_mut().dirty = true;
                true
            }
```

If either doesn't match, stop and re-read the full function before proceeding — do not guess at the diff.

- [ ] **Step 2: Replace the `"model"` arm**

```rust
            "model" => {
                let view = crate::config::llm_view::llm_runtime_view(config);
                let msg = match view.backend {
                    LlmBackend::Agent => {
                        if args.is_empty() {
                            "Agent mode: use /agent to switch agents.".to_string()
                        } else {
                            "Cannot set model in agent mode. Switch to provider backend first."
                                .to_string()
                        }
                    }
                    LlmBackend::Provider => {
                        if args.is_empty() {
                            format!(
                                "Active: {}:{}",
                                view.provider_cfg.provider, view.provider_cfg.model
                            )
                        } else {
                            config.llm.model = args.to_string();
                            self.rewire_backend(config, wakeup_proxy.clone());
                            format!("Model set to '{args}'.")
                        }
                    }
                };
                self.panel_mut()
                    .messages
                    .push(crate::llm::ChatMessage::assistant(msg));
                self.panel_mut().dirty = true;
                true
            }
```

Note: `config.llm.model = args.to_string();` is a write and stays raw — `view` was snapshotted before it and is never read again after the write in this arm, so there's no staleness.

- [ ] **Step 3: Replace the `"agent"` arm**

```rust
            "agent" => {
                let view = crate::config::llm_view::llm_runtime_view(config);
                let msg = match view.backend {
                    LlmBackend::Provider => {
                        "Provider mode active. Use /model to change models.".to_string()
                    }
                    LlmBackend::Agent => {
                        if args.is_empty() {
                            let name =
                                crate::config::llm_view::agent_display_name(view.agent.as_ref());
                            match name {
                                Some(n) => format!("Active agent: {n}"),
                                None => "No agent configured. Set llm.agent.command in config."
                                    .to_string(),
                            }
                        } else {
                            if let Some(agent_cfg) = config.llm.agent.as_mut() {
                                agent_cfg.command = args.to_string();
                            } else {
                                config.llm.agent = Some(crate::config::schema::AcpAgentConfig {
                                    command: args.to_string(),
                                    args: vec![],
                                    env: vec![],
                                    display_name: None,
                                });
                            }
                            self.acp_session = None;
                            self.rewire_backend(config, wakeup_proxy.clone());
                            format!("Agent set to '{args}'. Reconnecting...")
                        }
                    }
                };
                self.panel_mut()
                    .messages
                    .push(crate::llm::ChatMessage::assistant(msg));
                self.panel_mut().dirty = true;
                true
            }
```

Equivalence check (verify this while implementing, don't just take it on faith): old code returns `name = None` only when `config.llm.agent` is `None` (then prints "No agent configured..."); `agent_display_name(view.agent.as_ref())` returns `None` under the identical condition (`agent?` short-circuits), and `Some(...)` with the identical display_name-or-basename value otherwise. Same three writes (`agent_cfg.command = ...`, `config.llm.agent = Some(...)`) stay raw and untouched.

- [ ] **Step 4: Build and run the full test suite**

Run: `cargo build`
Expected: PASS, no new warnings.

Run: `cargo test`
Expected: PASS, same counts as before this task (111 across 3 suites, per the prior slice's baseline) plus Task 1's 3 new `agent_display_name` tests already counted in `--lib`.

- [ ] **Step 5: Manual sanity check**

Run the app, open the AI panel, run `/model` with no args (provider backend) and confirm it still prints `Active: <provider>:<model>`. If an agent is configured, switch to agent backend and run `/agent` with no args, confirming it still prints `Active agent: <name>` (or the "No agent configured" message if none is set).

- [ ] **Step 6: Commit**

```bash
git add src/app/ui/providers.rs
git commit -m "refactor: Migrate handle_slash_command's model/agent arms to LLM config view."
```

---

### Task 3: Deduplicate `build_panel_header`'s agent-name derivation

**Files:**
- Modify: `src/app/renderer/chat.rs` (function `build_panel_header`, the `LlmBackend::Agent` arm, lines ~451-464 as of commit `44a11d5`)

**Interfaces:**
- Consumes: `crate::config::llm_view::agent_display_name(Option<&AcpAgentConfig>) -> Option<&str>` (from Task 1)
- Produces: no new interfaces — `build_panel_header`'s signature is unchanged

- [ ] **Step 1: Confirm current code matches expectations**

Run: `sed -n '448,475p' src/app/renderer/chat.rs`

Expected:

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

If this doesn't match, stop and re-read the full function before proceeding.

- [ ] **Step 2: Replace the `Agent` arm's derivation with the shared helper**

```rust
        let view = crate::config::llm_view::llm_runtime_view(config);
        let (left_label, center_full) = match &view.backend {
            LlmBackend::Agent => {
                let name = crate::config::llm_view::agent_display_name(view.agent.as_ref())
                    .unwrap_or("agent");
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

Equivalence check (verify while implementing): old code's fallback when `agent` is `None` is the literal string `"agent"` (`cmd = ... unwrap_or("agent")`, then `name = basename("agent") = "agent"`); new code's `agent_display_name(None) = None`, then `.unwrap_or("agent")` — same literal fallback. When `agent` is `Some` with `display_name` set, both produce that display name. When `agent` is `Some` with `display_name` unset, both produce the basename of `agent.command`. All three cases match.

- [ ] **Step 3: Build and run the full test suite**

Run: `cargo build`
Expected: PASS, no new warnings.

Run: `cargo test`
Expected: PASS, same counts as after Task 2 (no new logic here, pure call-site substitution of an already-tested helper).

- [ ] **Step 4: Manual sanity check**

Run the app, open the AI panel with agent backend configured, confirm the header still shows `◈ <name>` (or `◈ agent` if unconfigured) exactly as before.

- [ ] **Step 5: Commit**

```bash
git add src/app/renderer/chat.rs
git commit -m "refactor: Deduplicate chat header agent-name derivation via shared helper."
```

## Self-Review

- **Spec coverage:** Task 1 adds the shared helper (tested standalone). Task 2 migrates every remaining raw-Config LLM *read* in the codebase (confirmed by the grep in this plan's header — the only lines left after Task 2 are the three writes, which are correctly out of scope). Task 3 removes the duplicated derivation the prior slice's final review flagged. Together these fully close the LLM domain — this claim is now backed by an explicit, reproducible grep, not an assumption.
- **Placeholder scan:** No TBD/TODO; every step has literal code and an explicit equivalence argument instead of "should behave the same."
- **Type consistency:** `agent_display_name(agent: Option<&AcpAgentConfig>) -> Option<&str>` is used identically in Tasks 2 and 3 with the same signature; `LlmRuntimeView`'s existing fields (`backend`, `agent`, `provider_cfg`) are unchanged from the prior slice.
