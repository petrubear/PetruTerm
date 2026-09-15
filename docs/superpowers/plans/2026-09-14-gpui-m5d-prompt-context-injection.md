# M5d — Prompt Context Injection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Inject skill-match, steering-file, shell-context, and attached-file text into both LLM
backends' prompts (direct-provider and ACP agent), and give the ACP agent native MCP server access
via the protocol's own `mcp_servers` field — for both the wgpu-based `petruterm` binary and the
gpui-based `gpui-petruterm` binary.

**Architecture:** A new, engine-agnostic `src/llm/prompt_context.rs` builds one text block
(`PromptAddendum`) from `SkillManager`/`SteeringManager`/`ShellContext`/attached files; both binaries'
direct-provider branch appends it to the system message, both ACP branches prepend it to the user's
prompt text (ACP has no system-message concept). A new `src/llm/mcp/config.rs::load_merged` consolidates
three near-identical global+local-merge copies (two in the wgpu binary, one in gpui_shell) into one
function; a new `to_acp_servers` maps this project's `McpConfig` to the ACP protocol's
`Vec<McpServer>`, passed into `AcpSession::connect`'s new `mcp_servers` parameter so the agent connects
to MCP servers itself — fully independent of `McpManager`'s existing proxy-based tool-calling, which
this plan does not touch.

**Tech Stack:** Rust, `agent-client-protocol`/`agent-client-protocol-schema` 0.11/0.12 (already a
workspace dependency), `tempfile` (already a dev-dependency, used for the new tests).

**Spec:** `docs/superpowers/specs/2026-09-14-gpui-m5d-prompt-context-injection-design.md`

## Global Constraints

- 400-line module limit applies to `src/llm/prompt_context.rs` (new) and any file under
  `src/gpui_shell/` this plan touches — it does NOT apply to `src/app/ui/mod.rs` (already 1846 lines,
  a pre-existing legacy file never subject to this migration's convention; edits there follow the
  file's own existing style, not a line-count target).
- `./scripts/ci-local.sh` must stay green (clippy `-D warnings`, `cargo fmt --check`, full test suite)
  — run `cargo fmt` proactively before every commit. `cargo audit`'s pre-existing
  `RUSTSEC-2026-0253` finding (against the `lru` crate) predates this milestone and is not a blocker
  if it's the only `ci-local.sh` failure.
- Tests are logic-only: no live-agent-process, no live-MCP-server-process, no live-subprocess tests —
  those are dogfooded by hand (spec §8). `SkillManager`/`SteeringManager` have no test-construction
  helper — their `load(cwd, include_local)` reads real filesystem paths, so tests use a `tempfile::
  TempDir` as `cwd` with `include_local: true`, writing fake `.petruterm/skills/`/`.petruterm/steering/`
  content into it (the global half of `load` always reads the real home dir, untouched by this — tests
  only assert on what the local half injected). `McpManager::load_global()` similarly reads real
  platform config paths and cannot be pointed at a temp dir — `load_merged`'s own tests assert only
  on locally-injected, uniquely-named keys, never on exact map equality, so they stay deterministic
  regardless of whatever the real test machine's global `mcp.json` (if any) contains.
  `ShellContext::load()` likewise reads a real cache-dir file — `prompt_context`'s own tests never
  assert on its presence or absence, only on the skill/steering/attached-file text they directly
  control.
- Commit format: `type: Message.` per `AGENTS.md` (type is one of `feat`/`fix`/`chore`/`refactor`).
- This is the first milestone in the whole gpui-migration session to touch `src/app/` (the original
  wgpu binary) and `src/llm/acp/` — both are explicitly in scope here.
- `McpManager` itself is not modified. The direct-provider path's existing MCP tool-proxying
  (`McpManager::all_tools_openai`/`call_tool`, routed through `execute_tool`) is untouched by every
  task in this plan.

---

## Task 1: The shared prompt-context builder

**Tier: cheap.** Pure new function with a fully-specified body (ported verbatim from existing,
working logic) plus fully-specified tests. No dependency on any other task in this plan.

**Files:**
- Create: `src/llm/prompt_context.rs`
- Modify: `src/lib.rs` (register the new module — see Step 1)

**Interfaces:**
- Produces: `prompt_context::PromptAddendum { text: String, matched_skill: Option<String> }` and
  `prompt_context::build_prompt_addendum(skill_manager: &SkillManager, steering_manager:
  &SteeringManager, active_skill_name: Option<&str>, user_content: &str, attached_files: &[PathBuf])
  -> PromptAddendum` — consumed by Task 5 (wgpu) and Task 6 (gpui_shell).

- [ ] **Step 1: Register the module**

Find `pub mod llm;` in `src/lib.rs` (or wherever `src/llm/mod.rs`'s own submodules are declared —
check `src/llm/mod.rs` directly: it already has a flat list of `pub mod`/`mod` lines for `skills`,
`steering`, `shell_context`, etc.). Add, alphabetically:

```rust
pub mod prompt_context;
```

to `src/llm/mod.rs`'s existing module-declaration list (read the file first to place it correctly
among the real current list — do not guess the surrounding lines).

- [ ] **Step 2: Write `prompt_context.rs`**

```rust
// M5d: the shared, engine-agnostic prompt-context builder -- skill-match
// instructions, steering-file rules, shell context, and attached-file
// contents, built once and consumed two ways: the direct-provider path
// appends `.text` to its system message; the ACP path prepends it to the
// user's own prompt text, since ACP has no system-message concept (see
// the M5d spec's Section 3). Ported verbatim from the wgpu build's own
// inline logic in `submit_ai_query` (`src/app/ui/mod.rs`, the block
// building `system_text` from steering/skill/shell-context/attached
// files) -- this is a pure extraction, not new logic.

use std::path::PathBuf;

use crate::llm::shell_context::ShellContext;
use crate::llm::skills::SkillManager;
use crate::llm::steering::SteeringManager;

/// Cap on a single attached file's injected content, and on the combined
/// total across all attached files (TD-030, ported verbatim).
const MAX_FILE_BYTES: usize = 512 * 1024;
const MAX_TOTAL_BYTES: usize = 1024 * 1024;

/// Extra context text to inject alongside a user's query. Empty `text`
/// means nothing applied -- callers skip appending/prepending in that
/// case rather than adding a stray blank block.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PromptAddendum {
    pub text: String,
    /// The skill that ended up active this turn, if any -- callers write
    /// this back into `ChatPanel::matched_skill`.
    pub matched_skill: Option<String>,
}

pub fn build_prompt_addendum(
    skill_manager: &SkillManager,
    steering_manager: &SteeringManager,
    active_skill_name: Option<&str>,
    user_content: &str,
    attached_files: &[PathBuf],
) -> PromptAddendum {
    let mut text = String::new();
    let mut matched_skill = None;

    // Steering files: global/project Markdown rules always active.
    if let Some(block) = steering_manager.context_block() {
        text.push_str(&format!("\n\n{block}"));
    }

    // Skill injection: match by query, or keep the conversation's active skill.
    let skill_match = if let Some(skill) = skill_manager.match_query(user_content) {
        let body = skill_manager.read_body(skill).ok();
        body.map(|b| (skill.name.clone(), b))
    } else if let Some(name) = active_skill_name {
        let found = skill_manager
            .skills()
            .iter()
            .find(|s| s.name == name)
            .cloned();
        found.and_then(|s| {
            skill_manager
                .read_body(&s)
                .ok()
                .map(|b| (name.to_string(), b))
        })
    } else {
        None
    };
    if let Some((skill_name, skill_body)) = skill_match {
        text.push_str(&format!(
            "\n\nThe following expert skill has been activated. \
             You MUST follow its instructions precisely. \
             All files referenced in the instructions (templates, guides, scripts) \
             are already included verbatim below — do NOT use file tools to read \
             them from disk, their content is already here:\n\n{skill_body}"
        ));
        matched_skill = Some(skill_name);
    }

    if let Some(ctx) = ShellContext::load() {
        text.push_str(&format!(
            "\n\nShell context:\n{}",
            ctx.format_for_system_message()
        ));
    }

    let mut total_bytes = 0usize;
    for path in attached_files {
        if total_bytes >= MAX_TOTAL_BYTES {
            break;
        }
        if let Ok(bytes) = std::fs::read(path) {
            let cap = bytes
                .len()
                .min(MAX_FILE_BYTES)
                .min(MAX_TOTAL_BYTES - total_bytes);
            let content = String::from_utf8_lossy(&bytes[..cap]);
            let name = path.display();
            text.push_str(&format!("\n\n--- File: {name} ---\n{content}"));
            if cap < bytes.len() {
                text.push_str("\n[... truncated — file exceeds size limit ...]");
            }
            total_bytes += cap;
        }
    }

    PromptAddendum {
        text,
        matched_skill,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_skill(dir: &std::path::Path, name: &str, description: &str, body: &str) {
        let skill_dir = dir.join(".petruterm/skills").join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n{body}"),
        )
        .unwrap();
    }

    fn write_steering(dir: &std::path::Path, filename: &str, content: &str) {
        let steering_dir = dir.join(".petruterm/steering");
        std::fs::create_dir_all(&steering_dir).unwrap();
        std::fs::write(steering_dir.join(filename), content).unwrap();
    }

    #[test]
    fn empty_managers_produce_no_skill_or_steering_text() {
        let skills = SkillManager::new();
        let steering = SteeringManager::new();
        let result = build_prompt_addendum(&skills, &steering, None, "hello", &[]);
        assert!(result.matched_skill.is_none());
        assert!(!result.text.contains("expert skill"));
        assert!(!result.text.contains("steering instructions"));
    }

    #[test]
    fn matching_skill_gets_injected_and_recorded() {
        let dir = TempDir::new().unwrap();
        write_skill(
            dir.path(),
            "git-helper",
            "Git branch expert",
            "Use `git switch`.",
        );
        let mut skills = SkillManager::new();
        skills.load(dir.path(), true);
        let steering = SteeringManager::new();

        let result =
            build_prompt_addendum(&skills, &steering, None, "skill git-helper please", &[]);
        assert_eq!(result.matched_skill, Some("git-helper".to_string()));
        assert!(result.text.contains("expert skill has been activated"));
        assert!(result.text.contains("Use `git switch`."));
    }

    #[test]
    fn active_skill_continues_when_no_new_match() {
        let dir = TempDir::new().unwrap();
        write_skill(
            dir.path(),
            "git-helper",
            "Git branch expert",
            "Use `git switch`.",
        );
        let mut skills = SkillManager::new();
        skills.load(dir.path(), true);
        let steering = SteeringManager::new();

        let result =
            build_prompt_addendum(&skills, &steering, Some("git-helper"), "what next?", &[]);
        assert_eq!(result.matched_skill, Some("git-helper".to_string()));
        assert!(result.text.contains("Use `git switch`."));
    }

    #[test]
    fn no_match_and_no_active_skill_leaves_matched_skill_none() {
        let dir = TempDir::new().unwrap();
        write_skill(
            dir.path(),
            "git-helper",
            "Git branch expert",
            "Use `git switch`.",
        );
        let mut skills = SkillManager::new();
        skills.load(dir.path(), true);
        let steering = SteeringManager::new();

        let result = build_prompt_addendum(&skills, &steering, None, "totally unrelated", &[]);
        assert!(result.matched_skill.is_none());
        assert!(!result.text.contains("expert skill"));
    }

    #[test]
    fn steering_block_gets_appended() {
        let dir = TempDir::new().unwrap();
        write_steering(dir.path(), "rules.md", "Be concise.");
        let skills = SkillManager::new();
        let mut steering = SteeringManager::new();
        steering.load(dir.path(), true);

        let result = build_prompt_addendum(&skills, &steering, None, "hi", &[]);
        assert!(result.text.contains("steering instructions"));
        assert!(result.text.contains("Be concise."));
    }

    #[test]
    fn attached_file_content_is_injected_with_header() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("notes.txt");
        std::fs::write(&file_path, "important notes").unwrap();
        let skills = SkillManager::new();
        let steering = SteeringManager::new();

        let result = build_prompt_addendum(&skills, &steering, None, "hi", &[file_path]);
        assert!(result.text.contains("--- File:"));
        assert!(result.text.contains("important notes"));
    }

    #[test]
    fn attached_file_over_cap_gets_truncated() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("big.txt");
        std::fs::write(&file_path, vec![b'x'; MAX_FILE_BYTES + 100]).unwrap();
        let skills = SkillManager::new();
        let steering = SteeringManager::new();

        let result = build_prompt_addendum(&skills, &steering, None, "hi", &[file_path]);
        assert!(result.text.contains("[... truncated"));
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --lib prompt_context:: -- --nocapture`
Expected: 7 tests pass (`empty_managers_produce_no_skill_or_steering_text`,
`matching_skill_gets_injected_and_recorded`, `active_skill_continues_when_no_new_match`,
`no_match_and_no_active_skill_leaves_matched_skill_none`, `steering_block_gets_appended`,
`attached_file_content_is_injected_with_header`, `attached_file_over_cap_gets_truncated`).

- [ ] **Step 4: Build, lint, format, verify**

Run: `cargo build`, `cargo test --lib` (expect all prior tests plus these 7 new ones passing), `cargo
fmt` then `cargo fmt --check`, `cargo clippy --all-features -- -D warnings`, `./scripts/ci-local.sh`
(the `cargo audit` step's pre-existing `RUSTSEC-2026-0253` finding is expected and not a blocker; every
step before it must pass cleanly). Run `wc -l src/llm/prompt_context.rs` — expect roughly 230-260
lines (well under 400).

- [ ] **Step 5: Commit**

```bash
git add src/llm/prompt_context.rs src/llm/mod.rs
git commit -m "feat: Add the shared prompt-context builder (M5d Task 1)."
```

---

## Task 2: MCP config consolidation

**Tier: cheap.** Pure new functions with fully-specified bodies and tests. No dependency on Task 1.

**Files:**
- Modify: `src/llm/mcp/config.rs`

**Interfaces:**
- Consumes: `McpConfig` (`= HashMap<String, McpServerConfig>`), `McpServerConfig { command: String,
  args: Vec<String>, env: HashMap<String, String> }`, `load_global() -> Result<McpConfig>`,
  `load_local(cwd: &Path) -> Result<McpConfig>` (all pre-existing, unmodified).
- Produces: `mcp::config::load_merged(cwd: &Path, trusted: bool) -> Result<McpConfig>` and
  `mcp::config::to_acp_servers(config: &McpConfig) -> Vec<agent_client_protocol::schema::McpServer>` —
  both consumed by Task 4 (wgpu) and Task 6 (gpui_shell). Note `trusted` is a plain `bool` parameter,
  not computed internally via `mcp::trust::is_trusted` — that function reads a real, un-mockable
  `~/.config/petruterm/mcp_trust.json` file, so keeping it a caller-supplied bool (matching
  `SkillManager::load`/`SteeringManager::load`'s own `include_local: bool` shape) is what makes
  `load_merged` testable for both the trusted and untrusted branches.

- [ ] **Step 1: Write `load_merged` and `to_acp_servers`**

Read `src/llm/mcp/config.rs`'s current exact content first (its real current line count is 188 — this
task's additions keep it well under 400) to confirm the surrounding code still matches what's described
here before editing. Add, right after the existing `load_local` function (before the `#[cfg(test)]
fn load_from_paths` block):

```rust
/// Load MCP config merged from global + project-local sources, matching the
/// merge policy three call sites (two in the wgpu binary, one in gpui_shell)
/// previously each duplicated inline. Global config always loads; local
/// config is included only when `trusted` is true. On a `load_global`
/// failure, returns `Err` (callers decide how to degrade — some treat this
/// as "keep whatever was already running," others as "connect with zero
/// MCP servers," which is why this doesn't collapse the error internally).
/// A `load_local` failure is logged and does not fail the whole call — the
/// global-only config is still returned, matching the more lenient of the
/// three call sites this consolidates.
pub fn load_merged(cwd: &Path, trusted: bool) -> Result<McpConfig> {
    let mut cfg = load_global()?;
    let local_path = cwd.join(".petruterm/mcp.json");
    if local_path.exists() {
        if trusted {
            match load_local(cwd) {
                Ok(local) => cfg.extend(local),
                Err(e) => log::warn!("MCP: failed to load local config: {e:#}"),
            }
        } else {
            log::info!(
                "Local MCP config found at {} but this directory is not trusted -- skipping. \
                 Use 'Trust local MCP' in the command palette to enable.",
                local_path.display()
            );
        }
    }
    Ok(cfg)
}

/// Map this project's own MCP config shape to the ACP protocol's server
/// list, for `NewSessionRequest::mcp_servers` -- the ACP agent connects to
/// and calls these servers' tools itself. This project's own `McpManager`
/// (used only for the direct-provider tool-calling path) is entirely
/// separate and untouched by this mapping.
pub fn to_acp_servers(config: &McpConfig) -> Vec<agent_client_protocol::schema::McpServer> {
    config
        .iter()
        .map(|(name, cfg)| {
            let env = cfg
                .env
                .iter()
                .map(
                    |(key, value)| agent_client_protocol::schema::EnvVariable {
                        name: key.clone(),
                        value: value.clone(),
                        meta: None,
                    },
                )
                .collect();
            agent_client_protocol::schema::McpServer::Stdio(
                agent_client_protocol::schema::McpServerStdio {
                    name: name.clone(),
                    command: cfg.command.clone().into(),
                    args: cfg.args.clone(),
                    env,
                    meta: None,
                },
            )
        })
        .collect()
}
```

- [ ] **Step 2: Write and run the tests**

Add, inside the existing `#[cfg(test)] mod tests { ... }` block (after the existing `env_vars_parsed`
test, before the closing `}`):

```rust
    #[test]
    fn load_merged_includes_local_when_trusted() {
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            ".petruterm/mcp.json",
            r#"{ "mcpServers": { "m5d-test-local-trusted": { "command": "local-cmd" } } }"#,
        );
        let cfg = load_merged(dir.path(), true).unwrap();
        assert_eq!(
            cfg.get("m5d-test-local-trusted").map(|c| c.command.as_str()),
            Some("local-cmd")
        );
    }

    #[test]
    fn load_merged_excludes_local_when_untrusted() {
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            ".petruterm/mcp.json",
            r#"{ "mcpServers": { "m5d-test-local-untrusted": { "command": "local-cmd" } } }"#,
        );
        let cfg = load_merged(dir.path(), false).unwrap();
        assert!(!cfg.contains_key("m5d-test-local-untrusted"));
    }

    #[test]
    fn load_merged_with_no_local_file_returns_global_only() {
        let dir = TempDir::new().unwrap();
        let cfg = load_merged(dir.path(), true).unwrap();
        assert!(!cfg.contains_key("m5d-test-should-never-exist"));
    }

    #[test]
    fn to_acp_servers_maps_stdio_shape() {
        let mut cfg = McpConfig::new();
        cfg.insert(
            "test-srv".to_string(),
            McpServerConfig {
                command: "npx".to_string(),
                args: vec!["-y".to_string(), "pkg".to_string()],
                env: HashMap::from([("FOO".to_string(), "bar".to_string())]),
            },
        );
        let servers = to_acp_servers(&cfg);
        assert_eq!(servers.len(), 1);
        match &servers[0] {
            agent_client_protocol::schema::McpServer::Stdio(s) => {
                assert_eq!(s.name, "test-srv");
                assert_eq!(s.command, std::path::PathBuf::from("npx"));
                assert_eq!(s.args, vec!["-y", "pkg"]);
                assert_eq!(s.env.len(), 1);
                assert_eq!(s.env[0].name, "FOO");
                assert_eq!(s.env[0].value, "bar");
            }
            other => panic!("expected Stdio variant, got {other:?}"),
        }
    }

    #[test]
    fn to_acp_servers_on_empty_config_is_empty() {
        let cfg = McpConfig::new();
        assert!(to_acp_servers(&cfg).is_empty());
    }
```

Run: `cargo test --lib mcp::config:: -- --nocapture`
Expected: all existing tests (`parse_valid_json`, `missing_file_returns_empty`,
`local_overrides_global`, `malformed_json_returns_err`, `env_vars_parsed`) plus the 6 new tests pass.

- [ ] **Step 3: Build, lint, format, verify**

Run: `cargo build`, `cargo test --lib`, `cargo fmt` then `cargo fmt --check`, `cargo clippy
--all-features -- -D warnings`, `./scripts/ci-local.sh` (same pre-existing `cargo audit` exception).
Run `wc -l src/llm/mcp/config.rs` — expect roughly 260-280 lines.

- [ ] **Step 4: Commit**

```bash
git add src/llm/mcp/config.rs
git commit -m "feat: Add MCP config merge consolidation and ACP server mapping (M5d Task 2)."
```

---

## Task 3: ACP session wiring — native `mcp_servers`

**Tier: standard.** Small diff, but touches a real async subprocess-handshake lifecycle shared by
both binaries for the first time — worth the extra care a fresh-eyes review gives it. No new
automated tests (the change is pure plumbing with no new decision logic; `connect`'s own behavior is
only verifiable against a live agent subprocess, per this project's established "no live-agent tests"
convention — dogfooded in Task 6/spec §8).

**Files:**
- Modify: `src/llm/acp/mod.rs`
- Modify: `src/llm/acp/session.rs`

**Interfaces:**
- Consumes: `agent_client_protocol::schema::{McpServer, NewSessionRequest}` (`NewSessionRequest`
  gains its list via the real, confirmed builder method `.mcp_servers(mcp_servers: Vec<McpServer>) ->
  Self`).
- Produces: `AcpSession::connect(cfg: &AcpAgentConfig, cwd: &Path, mcp_servers: Vec<McpServer>) ->
  Result<Self>` (signature change — was `connect(cfg, cwd)`) — consumed by Task 4 (wgpu) and Task 6
  (gpui_shell), both of which must update their own call sites in the same commit as Task 3 if this
  is executed as a single change, or (as planned here) tolerate a temporarily non-compiling
  intermediate state between Task 3 and Tasks 4/6 — since this plan executes tasks sequentially with a
  build-and-test gate at the end of every task, Task 3's own Step 4 build MUST pass on its own, meaning
  Task 3 has to update every current call site of `AcpSession::connect` itself (both binaries), even
  though the *behavior* those call sites gain (real mcp_servers content) is Task 4/6's job. Task 3
  updates the call sites to compile (passing `Vec::new()` for now); Tasks 4 and 6 replace that
  `Vec::new()` with real content.

- [ ] **Step 1: `src/llm/acp/mod.rs` — thread the parameter through `connect`**

Read the file's current exact content first (132 lines) to confirm `AcpSession::connect`'s body still
matches what's shown here. Change:

```rust
    pub async fn connect(cfg: &AcpAgentConfig, cwd: &Path) -> Result<Self> {
```

to:

```rust
    /// `mcp_servers` is passed straight into the ACP `session/new` request
    /// (`NewSessionRequest::mcp_servers`) -- the agent connects to and
    /// calls these servers' tools itself, independent of this project's
    /// own `McpManager` (used only by the direct-provider tool-calling
    /// path). Pass an empty `Vec` for no MCP access.
    pub async fn connect(
        cfg: &AcpAgentConfig,
        cwd: &Path,
        mcp_servers: Vec<agent_client_protocol::schema::McpServer>,
    ) -> Result<Self> {
```

And change the line that spawns `run_session`:

```rust
        let task = tokio::spawn(run_session(agent, cwd, prompt_rx, ready_tx));
```

to:

```rust
        let task = tokio::spawn(run_session(agent, cwd, mcp_servers, prompt_rx, ready_tx));
```

- [ ] **Step 2: `src/llm/acp/session.rs` — thread the parameter through `run_session`**

Read the file's current exact content first (340 lines) to confirm `run_session`'s signature and the
`NewSessionRequest::new(&cwd)` call site still match what's shown here. In the existing import block:

```rust
use agent_client_protocol::schema::{
    ContentBlock, CreateTerminalRequest, InitializeRequest, KillTerminalRequest,
    KillTerminalResponse, NewSessionRequest, PromptRequest, ProtocolVersion, ReadTextFileRequest,
    ReadTextFileResponse, ReleaseTerminalRequest, ReleaseTerminalResponse,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate, TerminalExitStatus, TerminalId,
    TerminalOutputRequest, TerminalOutputResponse, WaitForTerminalExitRequest,
    WaitForTerminalExitResponse, WriteTextFileRequest, WriteTextFileResponse,
};
```

add `McpServer` alphabetically (between `KillTerminalResponse` and `NewSessionRequest`):

```rust
use agent_client_protocol::schema::{
    ContentBlock, CreateTerminalRequest, InitializeRequest, KillTerminalRequest,
    KillTerminalResponse, McpServer, NewSessionRequest, PromptRequest, ProtocolVersion,
    ReadTextFileRequest, ReadTextFileResponse, ReleaseTerminalRequest, ReleaseTerminalResponse,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate, TerminalExitStatus, TerminalId,
    TerminalOutputRequest, TerminalOutputResponse, WaitForTerminalExitRequest,
    WaitForTerminalExitResponse, WriteTextFileRequest, WriteTextFileResponse,
};
```

Change the function signature:

```rust
pub(super) async fn run_session(
    agent: AcpAgent,
    cwd: PathBuf,
    mut prompt_rx: mpsc::Receiver<PromptMsg>,
    ready_tx: oneshot::Sender<Result<()>>,
) {
```

to:

```rust
pub(super) async fn run_session(
    agent: AcpAgent,
    cwd: PathBuf,
    mcp_servers: Vec<McpServer>,
    mut prompt_rx: mpsc::Receiver<PromptMsg>,
    ready_tx: oneshot::Sender<Result<()>>,
) {
```

Change the session-creation call:

```rust
            let sess = cx
                .send_request(NewSessionRequest::new(&cwd))
                .block_task()
                .await?;
```

to:

```rust
            let sess = cx
                .send_request(NewSessionRequest::new(&cwd).mcp_servers(mcp_servers))
                .block_task()
                .await?;
```

(`mcp_servers` is moved into the closure along with `cwd` — both are already captured by the
surrounding `async move` closure this call site lives inside, per the file's existing structure; no
extra `.clone()` needed since this is the only use of `mcp_servers`.)

- [ ] **Step 3: Update both binaries' existing call sites to keep the build green**

This step exists only to keep `cargo build` passing at the end of this task — it does NOT yet pass
real MCP server content (Tasks 4 and 6 do that). Three call sites need a third argument added:

In `src/app/ui/mod.rs`'s `spawn_acp_connect` (around line 37):

```rust
        let result = crate::llm::acp::AcpSession::connect(&agent_cfg, &cwd)
            .await
            .map_err(|e| format!("{e:#}"));
```

becomes:

```rust
        let result = crate::llm::acp::AcpSession::connect(&agent_cfg, &cwd, Vec::new())
            .await
            .map_err(|e| format!("{e:#}"));
```

In `src/gpui_shell/chat_panel/backend.rs`'s `spawn_acp_connect` (around line 86):

```rust
        let result = AcpSession::connect(&agent_cfg, &cwd)
            .await
            .map_err(|e| format!("{e:#}"));
```

becomes:

```rust
        let result = AcpSession::connect(&agent_cfg, &cwd, Vec::new())
            .await
            .map_err(|e| format!("{e:#}"));
```

Read each file's real current surrounding lines before editing to confirm exact placement — these are
the only two `AcpSession::connect` call sites in the whole codebase (confirm with
`grep -rn "AcpSession::connect" src/`), so no other file needs this placeholder update.

- [ ] **Step 4: Build, lint, format, verify**

Run: `cargo build` (must be clean — this is the main point of this step, confirming the signature
change compiles everywhere), `cargo test --lib` (expect no change in pass count — this task adds no
new tests), `cargo fmt` then `cargo fmt --check`, `cargo clippy --all-features -- -D warnings`,
`./scripts/ci-local.sh` (same pre-existing `cargo audit` exception). Run `wc -l src/llm/acp/mod.rs
src/llm/acp/session.rs` — expect roughly 135-140 and 342-345 lines respectively (both well under the
convention, though note this convention doesn't bind `src/app/` per the Global Constraints — it does
bind these `src/llm/` files).

- [ ] **Step 5: Commit**

```bash
git add src/llm/acp/mod.rs src/llm/acp/session.rs src/app/ui/mod.rs src/gpui_shell/chat_panel/backend.rs
git commit -m "feat: Thread native ACP mcp_servers through the session lifecycle (M5d Task 3).

Both binaries' AcpSession::connect call sites pass an empty Vec for now --
Tasks 4 and 6 replace it with real MCP server content."
```

---

## Task 4: wgpu — MCP config consolidation + `mcp_servers` wiring

**Tier: standard.** Multi-call-site refactor requiring real judgment to preserve each existing call
site's distinct error-handling behavior exactly (see the two "behavior preserved" notes below) while
consolidating duplicated logic and adding new functionality. Depends on Tasks 2 and 3.

**Files:**
- Modify: `src/app/ui/mod.rs`
- Modify: `src/app/ui/providers.rs`

**Interfaces:**
- Consumes: `mcp::config::{load_merged, to_acp_servers}` (Task 2), `AcpSession::connect(cfg, cwd,
  mcp_servers)` (Task 3).
- Produces: `spawn_acp_connect`'s new signature (adds `mcp_enabled: bool`) — internal to this file,
  no other task depends on it.

- [ ] **Step 1: `spawn_acp_connect` — construct real `mcp_servers` inside the spawned task**

Read `src/app/ui/mod.rs`'s current exact content around lines 26-41 first to confirm it still matches.
Change:

```rust
fn spawn_acp_connect(
    rt: &tokio::runtime::Runtime,
    agent_cfg: crate::config::schema::AcpAgentConfig,
    cwd: PathBuf,
    wakeup: EventLoopProxy<()>,
) -> tokio::sync::oneshot::Receiver<Result<crate::llm::acp::AcpSession, String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt.spawn(async move {
        let result = crate::llm::acp::AcpSession::connect(&agent_cfg, &cwd, Vec::new())
            .await
            .map_err(|e| format!("{e:#}"));
        let _ = tx.send(result);
        let _ = wakeup.send_event(());
    });
    rx
}
```

to:

```rust
fn spawn_acp_connect(
    rt: &tokio::runtime::Runtime,
    agent_cfg: crate::config::schema::AcpAgentConfig,
    cwd: PathBuf,
    mcp_enabled: bool,
    wakeup: EventLoopProxy<()>,
) -> tokio::sync::oneshot::Receiver<Result<crate::llm::acp::AcpSession, String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt.spawn(async move {
        // MCP config loading is real (blocking) file I/O -- done inside this
        // spawned task, not before `rt.spawn`, so `spawn_acp_connect` itself
        // stays non-blocking for its caller (the UI thread), per its own
        // existing doc comment.
        let mcp_servers = if mcp_enabled {
            let trusted = crate::llm::mcp::trust::is_trusted(&cwd);
            mcp_config::load_merged(&cwd, trusted)
                .map(|cfg| mcp_config::to_acp_servers(&cfg))
                .unwrap_or_else(|e| {
                    log::warn!("ACP: failed to load MCP config: {e:#}");
                    Vec::new()
                })
        } else {
            Vec::new()
        };
        let result = crate::llm::acp::AcpSession::connect(&agent_cfg, &cwd, mcp_servers)
            .await
            .map_err(|e| format!("{e:#}"));
        let _ = tx.send(result);
        let _ = wakeup.send_event(());
    });
    rx
}
```

(`mcp_config` is already imported at this file's top — `use crate::llm::mcp::config as mcp_config;`
— confirm this import still exists before editing; no new import needed for this step.)

- [ ] **Step 2: Update `spawn_acp_connect`'s call site in `UiManager::new`**

Read the surrounding code around line 305 first to confirm it still matches. Change:

```rust
                        spawn_acp_connect(&tokio_rt, agent_cfg.clone(), cwd, wakeup_proxy.clone())
```

to:

```rust
                        spawn_acp_connect(
                            &tokio_rt,
                            agent_cfg.clone(),
                            cwd,
                            view.enabled,
                            wakeup_proxy.clone(),
                        )
```

(`view` — the `LlmRuntimeView` — is already in scope at this call site, confirmed by the surrounding
`if view.enabled { match view.backend { ... } }` structure this line lives inside.)

- [ ] **Step 3: Consolidate `UiManager::new`'s own inline MCP merge**

Read the surrounding code around lines 343-372 first to confirm it still matches. Change:

```rust
        let mcp_manager = if view.enabled {
            let mut mgr = McpManager::new();
            // Always load global MCP servers (installed by the user deliberately).
            if let Ok(mut cfg) = mcp_config::load_global() {
                // Load project-local MCP only if this cwd has been explicitly trusted.
                // This prevents a malicious repo's .petruterm/mcp.json from spawning
                // arbitrary processes when the directory is opened (AUDIT-SEC-02).
                if let Ok(cwd) = std::env::current_dir() {
                    let local_path = cwd.join(".petruterm/mcp.json");
                    if local_path.exists() {
                        if crate::llm::mcp::trust::is_trusted(&cwd) {
                            if let Ok(local) = mcp_config::load_local(&cwd) {
                                cfg.extend(local);
                            }
                        } else {
                            log::info!(
                                "Local MCP config found at {}/.petruterm/mcp.json but this \
                                 directory is not trusted — skipping. Use 'Trust local MCP' \
                                 in the command palette to enable.",
                                cwd.display()
                            );
                        }
                    }
                }
                if !cfg.is_empty() {
                    let errors = tokio_rt.block_on(mgr.start_all(&cfg));
                    for (name, err) in &errors {
                        log::warn!("MCP server '{name}' failed to start: {err:#}");
                    }
                }
            }
            std::sync::Arc::new(mgr)
        } else {
            std::sync::Arc::new(McpManager::new())
        };
```

to:

```rust
        let mcp_manager = if view.enabled {
            let mut mgr = McpManager::new();
            // Behavior preserved exactly: if `std::env::current_dir()` fails, or
            // `load_merged` returns `Err` (global config failed to load), `mgr`
            // stays empty -- the whole inner block is simply skipped, same as
            // the original nested `if let Ok(...)` chain this replaces.
            if let Ok(cwd) = std::env::current_dir() {
                let trusted = crate::llm::mcp::trust::is_trusted(&cwd);
                if let Ok(cfg) = mcp_config::load_merged(&cwd, trusted) {
                    if !cfg.is_empty() {
                        let errors = tokio_rt.block_on(mgr.start_all(&cfg));
                        for (name, err) in &errors {
                            log::warn!("MCP server '{name}' failed to start: {err:#}");
                        }
                    }
                }
            }
            std::sync::Arc::new(mgr)
        } else {
            std::sync::Arc::new(McpManager::new())
        };
```

- [ ] **Step 4: Update `spawn_acp_connect`'s call site in `providers.rs::rewire_backend`**

Read `src/app/ui/providers.rs`'s current exact content around line 74 first to confirm it still
matches. Change:

```rust
                if let Some(agent_cfg) = view.agent {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    self.acp_pending_connect = Some(super::spawn_acp_connect(
                        &self.tokio_rt,
                        agent_cfg,
                        cwd,
                        wakeup_proxy,
                    ));
                } else {
```

to:

```rust
                if let Some(agent_cfg) = view.agent {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    self.acp_pending_connect = Some(super::spawn_acp_connect(
                        &self.tokio_rt,
                        agent_cfg,
                        cwd,
                        view.enabled,
                        wakeup_proxy,
                    ));
                } else {
```

(`view` is already in scope — `rewire_backend`'s own first line is `let view =
crate::config::llm_view::llm_runtime_view(config);`, confirmed by the surrounding code.)

- [ ] **Step 5: Consolidate `reload_mcp`'s own inline MCP merge**

Read `providers.rs`'s current exact content of `reload_mcp` (lines ~7-38) first to confirm it still
matches. Change:

```rust
    pub fn reload_mcp(&mut self, cwd: &std::path::Path) {
        let mut cfg = match mcp_config::load_global() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("MCP hot-reload: failed to load global config: {e:#}");
                return;
            }
        };
        // Include local config only if the cwd is trusted (AUDIT-SEC-02).
        let local_path = cwd.join(".petruterm/mcp.json");
        if local_path.exists() {
            if crate::llm::mcp::trust::is_trusted(cwd) {
                match mcp_config::load_local(cwd) {
                    Ok(local) => cfg.extend(local),
                    Err(e) => log::warn!("MCP hot-reload: failed to load local config: {e:#}"),
                }
            } else {
                log::info!(
                    "MCP hot-reload: local config at {} not trusted, skipping.",
                    local_path.display()
                );
            }
        }
        let mut mgr = McpManager::new();
        let errors = self.tokio_rt.block_on(mgr.start_all(&cfg));
        for (name, err) in &errors {
            log::warn!("MCP hot-reload: server '{name}' failed to start: {err:#}");
        }
        let connected = mgr.connected_count();
        self.mcp_manager = std::sync::Arc::new(mgr);
        self.chat_panel.mcp_connected = connected;
        log::info!("MCP hot-reloaded: {connected} server(s) connected.");
    }
```

to:

```rust
    pub fn reload_mcp(&mut self, cwd: &std::path::Path) {
        // Behavior preserved exactly: on a `load_merged` `Err` (global config
        // failed to load), this returns early WITHOUT touching
        // `self.mcp_manager` -- a broken hot-reload keeps whatever MCP
        // servers were already running, same as the original early-return
        // this replaces. This differs deliberately from `UiManager::new`'s
        // own call site above (Step 3), which treats the same error as
        // "start with zero MCP servers" -- that's correct there because
        // there's no prior state to preserve at construction time.
        let trusted = crate::llm::mcp::trust::is_trusted(cwd);
        let cfg = match mcp_config::load_merged(cwd, trusted) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("MCP hot-reload: failed to load global config: {e:#}");
                return;
            }
        };
        let mut mgr = McpManager::new();
        let errors = self.tokio_rt.block_on(mgr.start_all(&cfg));
        for (name, err) in &errors {
            log::warn!("MCP hot-reload: server '{name}' failed to start: {err:#}");
        }
        let connected = mgr.connected_count();
        self.mcp_manager = std::sync::Arc::new(mgr);
        self.chat_panel.mcp_connected = connected;
        log::info!("MCP hot-reloaded: {connected} server(s) connected.");
    }
```

- [ ] **Step 6: Build, lint, format, verify**

Run: `cargo build`, `cargo test --lib` (no count change expected — this task adds no new tests, it's
consumer wiring), `cargo fmt` then `cargo fmt --check`, `cargo clippy --all-features -- -D warnings`,
`./scripts/ci-local.sh` (same pre-existing `cargo audit` exception).

Dogfood note (cannot be verified from this sandbox): with a real `llm.agent` config and a configured
`mcp.json`, the ACP agent should report the configured MCP servers' tools as available — see spec §8.

- [ ] **Step 7: Commit**

```bash
git add src/app/ui/mod.rs src/app/ui/providers.rs
git commit -m "feat: Wire native MCP server access into the wgpu ACP branch (M5d Task 4).

Consolidates three near-identical global+local mcp.json merge copies
(two in this file, one in gpui_shell, addressed in Task 6) into
mcp::config::load_merged."
```

---

## Task 5: wgpu — prompt-context wiring in `submit_ai_query`

**Tier: standard.** Refactors existing, working logic into a shared function call (must preserve
exact resulting text) and adds new logic to the ACP branch (which currently has none) — real
judgment required to get both right without regressing the working direct-provider path. Depends on
Task 1.

**Files:**
- Modify: `src/app/ui/mod.rs`

**Interfaces:**
- Consumes: `prompt_context::{PromptAddendum, build_prompt_addendum}` (Task 1).

- [ ] **Step 1: Compute the addendum once, before both branches**

Read `submit_ai_query`'s current exact content (lines 794 onward) first to confirm it still matches.
Find:

```rust
    pub fn submit_ai_query(&mut self, wakeup_proxy: EventLoopProxy<()>, cwd: PathBuf) {
        // Canonicalize once — on macOS /var is a symlink to /private/var; without this
        // execute_tool's canon.starts_with(cwd) check always fails (TD-029).
        let cwd = cwd.canonicalize().unwrap_or(cwd);
        let panel_id = 0usize;
        let Some(user_content) = self.panel_mut().submit_input() else {
            return;
        };

        // ── ACP agent backend ─────────────────────────────────────────────────
        if self.acp_session.is_some() {
```

and insert, right after `let Some(user_content) = ...` and before the `// ── ACP agent backend`
comment:

```rust
        let addendum = crate::llm::prompt_context::build_prompt_addendum(
            &self.skill_manager,
            &self.steering_manager,
            self.panel().matched_skill.as_deref(),
            &user_content,
            &self.panel().attached_files,
        );
        if let Some(name) = addendum.matched_skill.clone() {
            self.panel_mut().matched_skill = Some(name);
        }

```

(`self.panel()`'s two borrows here are both released by the time `build_prompt_addendum` returns an
owned `PromptAddendum` — no explicit `.clone()` of `matched_skill`/`attached_files` needed before the
call, matching the same direct-field-access shape Task 6's gpui_shell version uses below.)

- [ ] **Step 2: Use the addendum in the ACP branch**

Find, still inside the `if self.acp_session.is_some() { ... }` block:

```rust
            let send_result = self.acp_session.as_mut().unwrap().try_send_prompt(
                user_content,
                ai_mpsc_tx,
                term_mpsc_tx,
            );
```

Change to:

```rust
            let prompt_text = if addendum.text.is_empty() {
                user_content
            } else {
                format!("{}\n\n{user_content}", addendum.text.trim_start())
            };
            let send_result = self.acp_session.as_mut().unwrap().try_send_prompt(
                prompt_text,
                ai_mpsc_tx,
                term_mpsc_tx,
            );
```

(`.trim_start()` on `addendum.text` here specifically: the addendum's own text starts with a leading
`"\n\n"` when non-empty — harmless as a separator after the direct-provider path's non-empty base
system prompt, Step 3 below, but would leave stray leading blank lines at the very start of the ACP
prompt text, which has no preceding text to separate from.)

- [ ] **Step 3: Replace the direct-provider branch's inline block with the addendum**

Find the block (already computed above by Step 1, so this step only replaces the text-construction
part, not the skill-matching part which Step 1 already extracted):

```rust
        self.panel_mut().context_window = provider.context_window();

        let mut system_text = self.system_prompt.clone();

        // Steering files: global/project Markdown rules always active.
        if let Some(block) = self.steering_manager.context_block() {
            system_text.push_str(&format!("\n\n{block}"));
        }

        // Skill injection (D-4): match by query, or keep the panel's active skill.
        let active_skill_name = self.panel().matched_skill.clone();
        let skill_match = {
            if let Some(skill) = self.skill_manager.match_query(&user_content) {
                let body = self.skill_manager.read_body(skill).ok();
                body.map(|b| (skill.name.clone(), b))
            } else if let Some(name) = &active_skill_name {
                // No new match — reuse the skill active in this conversation.
                let found = self
                    .skill_manager
                    .skills()
                    .iter()
                    .find(|s| &s.name == name)
                    .cloned();
                found.and_then(|s| {
                    self.skill_manager
                        .read_body(&s)
                        .ok()
                        .map(|b| (name.clone(), b))
                })
            } else {
                None
            }
        };
        if let Some((skill_name, skill_body)) = skill_match {
            system_text.push_str(&format!(
                "\n\nThe following expert skill has been activated. \
                 You MUST follow its instructions precisely. \
                 All files referenced in the instructions (templates, guides, scripts) \
                 are already included verbatim below — do NOT use file tools to read \
                 them from disk, their content is already here:\n\n{skill_body}"
            ));
            self.panel_mut().matched_skill = Some(skill_name);
        }

        if let Some(ctx) = ShellContext::load() {
            system_text.push_str(&format!(
                "\n\nShell context:\n{}",
                ctx.format_for_system_message()
            ));
        }

        // Inject attached file contents — capped at 512 KB/file and 1 MB total (TD-030).
        const MAX_FILE_BYTES: usize = 512 * 1024;
        const MAX_TOTAL_BYTES: usize = 1024 * 1024;
        let mut total_bytes = 0usize;
        let attached: Vec<_> = self.panel().attached_files.clone();
        for path in &attached {
            if total_bytes >= MAX_TOTAL_BYTES {
                break;
            }
            if let Ok(bytes) = std::fs::read(path) {
                let cap = bytes
                    .len()
                    .min(MAX_FILE_BYTES)
                    .min(MAX_TOTAL_BYTES - total_bytes);
                let content = String::from_utf8_lossy(&bytes[..cap]);
                let name = path.display();
                system_text.push_str(&format!("\n\n--- File: {name} ---\n{content}"));
                if cap < bytes.len() {
                    system_text.push_str("\n[... truncated — file exceeds size limit ...]");
                }
                total_bytes += cap;
            }
        }
```

Replace the entire block above with:

```rust
        self.panel_mut().context_window = provider.context_window();

        let mut system_text = self.system_prompt.clone();
        system_text.push_str(&addendum.text);
```

(This is a pure refactor: `addendum.text` is byte-for-byte the same steering+skill+shell-context+
attached-files text the deleted block used to build directly into `system_text` — verified by Task 1's
own tests exercising each piece in isolation, and by this step preserving the exact same
`self.system_prompt.clone()` base and the exact same `push_str` composition order.)

- [ ] **Step 4: Remove now-dead imports if any**

After Step 3's edit, `ShellContext` may no longer be referenced directly in this file (it's now only
used inside `prompt_context.rs`). Run `cargo build` (Step 6 below does this anyway, but check the
compiler's own `unused import` warning here first) — if `use crate::llm::shell_context::ShellContext;`
at this file's top is now unused, remove that one `use` line. Do not remove any other import without
confirming via a real build warning first (this file has ~30 imports; only touch the one this specific
edit affects).

- [ ] **Step 5: Build, lint, format, verify**

Run: `cargo build`, `cargo test --lib` (no count change expected), `cargo fmt` then `cargo fmt
--check`, `cargo clippy --all-features -- -D warnings`, `./scripts/ci-local.sh` (same pre-existing
`cargo audit` exception).

Dogfood note (cannot be verified from this sandbox): with a skill installed and a matching query, both
the direct-provider response (already worked before this refactor — confirm it STILL works, i.e. no
regression) and a real ACP agent's response (new) should reflect the skill's instructions — see spec
§8.

- [ ] **Step 6: Commit**

```bash
git add src/app/ui/mod.rs
git commit -m "refactor: Wire the shared prompt-context builder into wgpu's submit_ai_query (M5d Task 5).

Direct-provider branch: pure refactor, same resulting system-prompt text.
ACP branch: gains skill/steering/shell-context/attached-file injection
for the first time, prepended to the prompt text."
```

---

## Task 6: gpui_shell — prompt-context + `mcp_servers` wiring

**Tier: standard.** Closes a real, previously-total gap (gpui_shell's direct-provider path currently
injects nothing) and requires restructuring the call site to route around a real ownership
constraint: `ChatPanelView::submit` has no access to `GpuiShellRoot::{skill_manager,
steering_manager}` (different struct entirely) — the addendum must be built at the `GpuiShellRoot`-
level call site and passed into `submit` as a parameter. Depends on Tasks 1, 2, and 3.

**Files:**
- Modify: `src/gpui_shell/chat_panel/stream.rs`
- Modify: `src/gpui_shell/chat_panel/backend.rs`

**Interfaces:**
- Consumes: `prompt_context::{PromptAddendum, build_prompt_addendum}` (Task 1), `mcp::config::
  {load_merged, to_acp_servers}` (Task 2), `AcpSession::connect(cfg, cwd, mcp_servers)` (Task 3).
- Produces: `ChatPanelView::submit`'s new signature (adds a `PromptAddendum` parameter) — internal to
  this file, no other task depends on it.

- [ ] **Step 1: `backend.rs` — construct real `mcp_servers` in `spawn_acp_connect`**

Read `src/gpui_shell/chat_panel/backend.rs`'s current exact content first (92 lines) to confirm it
still matches. Change:

```rust
    pub fn rewire_backend(&mut self, config: &Config, tokio_rt: &tokio::runtime::Runtime) {
        self.acp_pending_connect = None;
        let view = crate::config::llm_view::llm_runtime_view(config);
        match view.backend {
            LlmBackend::Provider => {
                self.acp_session = None;
                self.rewire_provider(&config.llm);
            }
            LlmBackend::Agent => {
                self.llm_provider = None;
                self.llm_init_error = None;
                self.acp_session = None;
                if let Some(agent_cfg) = config.llm.agent.clone() {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    self.acp_pending_connect = Some(spawn_acp_connect(tokio_rt, agent_cfg, cwd));
                } else {
                    self.llm_init_error =
                        Some("llm.agent config is required when backend = \"agent\"".into());
                }
            }
        }
    }
```

to:

```rust
    pub fn rewire_backend(&mut self, config: &Config, tokio_rt: &tokio::runtime::Runtime) {
        self.acp_pending_connect = None;
        let view = crate::config::llm_view::llm_runtime_view(config);
        match view.backend {
            LlmBackend::Provider => {
                self.acp_session = None;
                self.rewire_provider(&config.llm);
            }
            LlmBackend::Agent => {
                self.llm_provider = None;
                self.llm_init_error = None;
                self.acp_session = None;
                if let Some(agent_cfg) = config.llm.agent.clone() {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    self.acp_pending_connect = Some(spawn_acp_connect(
                        tokio_rt,
                        agent_cfg,
                        cwd,
                        config.llm.enabled,
                    ));
                } else {
                    self.llm_init_error =
                        Some("llm.agent config is required when backend = \"agent\"".into());
                }
            }
        }
    }
```

And change:

```rust
fn spawn_acp_connect(
    rt: &tokio::runtime::Runtime,
    agent_cfg: crate::config::schema::AcpAgentConfig,
    cwd: PathBuf,
) -> tokio::sync::oneshot::Receiver<Result<AcpSession, String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt.spawn(async move {
        let result = AcpSession::connect(&agent_cfg, &cwd, Vec::new())
            .await
            .map_err(|e| format!("{e:#}"));
        let _ = tx.send(result);
    });
    rx
}
```

to:

```rust
fn spawn_acp_connect(
    rt: &tokio::runtime::Runtime,
    agent_cfg: crate::config::schema::AcpAgentConfig,
    cwd: PathBuf,
    mcp_enabled: bool,
) -> tokio::sync::oneshot::Receiver<Result<AcpSession, String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt.spawn(async move {
        let mcp_servers = if mcp_enabled {
            let trusted = crate::llm::mcp::trust::is_trusted(&cwd);
            crate::llm::mcp::config::load_merged(&cwd, trusted)
                .map(|cfg| crate::llm::mcp::config::to_acp_servers(&cfg))
                .unwrap_or_else(|e| {
                    log::warn!("ACP: failed to load MCP config: {e:#}");
                    Vec::new()
                })
        } else {
            Vec::new()
        };
        let result = AcpSession::connect(&agent_cfg, &cwd, mcp_servers)
            .await
            .map_err(|e| format!("{e:#}"));
        let _ = tx.send(result);
    });
    rx
}
```

- [ ] **Step 2: `stream.rs` — widen `submit`'s signature to accept the addendum**

Read `src/gpui_shell/chat_panel/stream.rs`'s current exact content first (347 lines) to confirm it
still matches. Change:

```rust
    pub fn submit(&mut self, tokio_rt: &tokio::runtime::Runtime, cx: &mut Context<GpuiShellRoot>) {
        let Some(user_content) = self.panel.submit_input() else {
            return;
        };

        if self.acp_session.is_some() {
```

to:

```rust
    pub fn submit(
        &mut self,
        addendum: crate::llm::prompt_context::PromptAddendum,
        tokio_rt: &tokio::runtime::Runtime,
        cx: &mut Context<GpuiShellRoot>,
    ) {
        let Some(user_content) = self.panel.submit_input() else {
            return;
        };
        if let Some(name) = addendum.matched_skill.clone() {
            self.panel.matched_skill = Some(name);
        }

        if self.acp_session.is_some() {
```

- [ ] **Step 3: Use the addendum in the ACP branch**

Find:

```rust
            let terminal_tx = self.acp_terminal_tx.clone();
            let send_result = self.acp_session.as_mut().unwrap().try_send_prompt(
                user_content,
                bridge_tx,
                terminal_tx,
            );
```

Change to:

```rust
            let terminal_tx = self.acp_terminal_tx.clone();
            let prompt_text = if addendum.text.is_empty() {
                user_content
            } else {
                format!("{}\n\n{user_content}", addendum.text.trim_start())
            };
            let send_result = self.acp_session.as_mut().unwrap().try_send_prompt(
                prompt_text,
                bridge_tx,
                terminal_tx,
            );
```

- [ ] **Step 4: Use the addendum in the direct-provider branch**

Find:

```rust
        let system_prompt = format!(
            "{}\n\n{}",
            crate::config::load_system_prompt(),
            crate::llm::agent_action::system_prompt_instructions()
        );
        let mut messages = vec![ChatMessage::system(system_prompt)];
```

Change to:

```rust
        let mut system_prompt = crate::config::load_system_prompt();
        system_prompt.push_str(&addendum.text);
        system_prompt.push('\n');
        system_prompt.push('\n');
        system_prompt.push_str(crate::llm::agent_action::system_prompt_instructions());
        let mut messages = vec![ChatMessage::system(system_prompt)];
```

(This matches wgpu's own Task 5 ordering exactly: base system prompt, then the addendum's text — which
carries its own leading `"\n\n"` separator when non-empty — then the agent-action instructions with an
explicit blank-line separator.)

- [ ] **Step 5: Build the addendum at the call site and update `submit`'s call**

Read the surrounding code of `handle_chat_composer_submit` (in this same file, around line 231) first
to confirm it still matches. Change:

```rust
    fn handle_chat_composer_submit(&mut self, cx: &mut Context<Self>) {
        if !self.chat.panel.is_idle() {
            return;
        }
        let text = self.chat.composer.read(cx).content().trim().to_string();
        if text.is_empty() {
            return;
        }
        self.chat
            .composer
            .update(cx, |input, cx| input.set_content("", cx));
        if text.starts_with('/') {
            self.handle_slash_command(&text, cx);
        } else {
            self.chat.panel.set_input(text);
            self.chat.submit(&self.tokio_rt, cx);
        }
    }
```

to:

```rust
    fn handle_chat_composer_submit(&mut self, cx: &mut Context<Self>) {
        if !self.chat.panel.is_idle() {
            return;
        }
        let text = self.chat.composer.read(cx).content().trim().to_string();
        if text.is_empty() {
            return;
        }
        self.chat
            .composer
            .update(cx, |input, cx| input.set_content("", cx));
        if text.starts_with('/') {
            self.handle_slash_command(&text, cx);
        } else {
            // `ChatPanelView` has no access to `skill_manager`/`steering_manager`
            // (they live on `GpuiShellRoot`, a different struct) -- built here,
            // where both are in scope, and passed down into `submit` rather
            // than reached for from inside it.
            let addendum = crate::llm::prompt_context::build_prompt_addendum(
                &self.skill_manager,
                &self.steering_manager,
                self.chat.panel.matched_skill.as_deref(),
                &text,
                &self.chat.panel.attached_files,
            );
            self.chat.panel.set_input(text);
            self.chat.submit(addendum, &self.tokio_rt, cx);
        }
    }
```

- [ ] **Step 6: Build, lint, format, verify**

Run: `cargo build`, `cargo test --lib` (no count change expected — this task is consumer wiring, no
new logic beyond what Tasks 1-2's own tests already cover), `cargo fmt` then `cargo fmt --check`,
`cargo clippy --all-features -- -D warnings`, `./scripts/ci-local.sh` (same pre-existing `cargo audit`
exception). Run `wc -l src/gpui_shell/chat_panel/stream.rs src/gpui_shell/chat_panel/backend.rs` —
expect roughly 355-365 and 110-120 lines respectively (both under 400).

Dogfood note (cannot be verified from this sandbox): reproduce the same checks as Task 5's dogfood
note, but in gpui-petruterm specifically — both a skill match and (with a configured `mcp.json`) an
ACP agent's MCP tool access should now work in this binary too, closing the exact gap the `/skills`
slash command's "not wired" text originally surfaced. See spec §8 for the full checklist.

- [ ] **Step 7: Commit**

```bash
git add src/gpui_shell/chat_panel/stream.rs src/gpui_shell/chat_panel/backend.rs
git commit -m "feat: Wire prompt-context injection and native MCP access into gpui_shell (M5d Task 6).

Closes the real, total gap this milestone was scoped to fix: gpui_shell's
direct-provider path previously injected nothing beyond the base system
prompt; both its backends now get skill/steering/shell-context/attached-
file text, and the ACP backend gets native MCP server access."
```

---

## Manual testing checklist (spec §8, reproduce after all 6 tasks land)

- With a skill installed and a matching query: both backends' responses in both binaries show the
  skill's instructions took effect.
- With an MCP server configured in `mcp.json`: the ACP agent (e.g. Claude Code's own ACP adapter)
  reports that server's tools as available and can call them, in both binaries, without `McpManager`
  itself being involved (check the agent's own tool-list or its logs, not petruterm's `/mcp` command,
  which only reports `McpManager`'s own state — the direct-provider path's view).
- Attaching a file via the composer's file picker and asking about it: both backends' responses in
  both binaries reflect the file's content.
- Confirm no regression in the direct-provider path's pre-existing behavior in wgpu (skill match,
  steering block, shell context, attached files) — Task 5 was a refactor of already-working code.
