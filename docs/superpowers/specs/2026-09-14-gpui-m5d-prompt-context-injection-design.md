# M5d — Prompt Context Injection (Skills, Steering, Shell Context, MCP) Design

## 1. Overview

Discovered during M5a dogfood: the gpui chat panel's `/skills` command reports "not wired," and
investigation showed the gap is real and larger than expected. Neither binary injects skill/steering/
shell-context/attached-file text into an ACP agent's prompt — the wgpu build's `submit_ai_query`
(`src/app/ui/mod.rs:794`) only builds that text for its **direct-provider** branch (lines ~856-931); its
own ACP branch (lines 803-843) sends the raw user text with nothing added. gpui_shell is missing this
for *both* backends: its direct-provider path (`chat_panel/stream.rs::submit`) sends only
`crate::config::load_system_prompt()`, no skill/steering/shell-context/attached-files at all.

Separately, MCP tool access for an ACP agent is entirely unwired in both binaries. The ACP protocol
crate this project vendors (`agent-client-protocol-schema` 0.12, via `agent-client-protocol`/
`agent-client-protocol-tokio` 0.11) has a native mechanism for this — `NewSessionRequest.mcp_servers:
Vec<McpServer>` — that neither binary currently populates (`src/llm/acp/session.rs:296` calls
`NewSessionRequest::new(&cwd)` with an empty list).

This milestone closes both gaps for both binaries: skill/steering/shell-context/attached-file text
gets built once by a new shared, engine-agnostic function and consumed by both backends' prompt paths
(appended to the system message for direct-provider, prepended to the user's own prompt text for ACP,
since ACP has no system-message concept); MCP access for an ACP agent is wired through the protocol's
native `mcp_servers` field, kept fully independent from the direct-provider path's existing
`McpManager`-proxied tool-calling (which stays exactly as it is today).

**Scope boundary, confirmed with the user:** the direct-provider path's MCP tool-calling
(`McpManager::all_tools_openai`/`call_tool`, proxied through `execute_tool`) is untouched by this
milestone. ACP's MCP access is a parallel, independent mechanism — the agent connects to MCP servers
itself and calls their tools directly over the MCP protocol; petruterm does not proxy for ACP.

## 2. Global Constraints

- Shared logic goes in `src/llm/` (engine-agnostic), consumed by both `src/app/ui/mod.rs` (wgpu) and
  `src/gpui_shell/chat_panel/{stream,backend}.rs` (gpui). This milestone is the first in the whole
  migration to touch `src/app/`/`src/llm/acp/` — both are explicitly in scope here (unlike every prior
  gpui-migration milestone, which left them untouched).
- `McpManager` itself is not modified to retain `McpConfig` after `start_all()` — the ACP connect path
  reloads `mcp.json` fresh via a new consolidated loader (Section 4), rather than extracting configs
  back out of `McpManager`.
- The `mcp_servers` list and the prompt-context addendum are both gated on `config.llm.enabled` — the
  same gate `McpManager`'s own construction-time loading already uses.
- 400-line module limit per file (project-wide convention); `cargo fmt`/`clippy -D warnings`/full test
  suite stay green; tests are logic-only (no live-agent, no live-MCP-server-process, no live-subprocess
  tests) — dogfooded by hand, matching this project's established testing convention.
- Commit format: `type: Message.` per `AGENTS.md`.

## 3. The shared prompt-context builder

**New file:** `src/llm/prompt_context.rs`.

**Signature:**

```rust
pub struct PromptAddendum {
    /// Empty if nothing matched/applies — callers skip appending/prepending in that case.
    pub text: String,
    /// The skill that ended up active this turn, if any — callers write this back into
    /// `ChatPanel::matched_skill`.
    pub matched_skill: Option<String>,
}

pub fn build_prompt_addendum(
    skill_manager: &SkillManager,
    steering_manager: &SteeringManager,
    active_skill_name: Option<&str>,
    user_content: &str,
    attached_files: &[PathBuf],
) -> PromptAddendum
```

**Behavior**, ported verbatim from wgpu's `submit_ai_query` (lines ~864-931), unchanged in substance:

1. Steering block: `steering_manager.context_block()`, appended first if present.
2. Skill match: `skill_manager.match_query(user_content)`; if none, and `active_skill_name` is
   `Some`, the currently-active skill continues (looked up again by name so `read_body` is always
   called against a fresh read). The matched skill's instructions get the same "you MUST follow its
   instructions precisely... do NOT use file tools to read them" framing wgpu already uses.
3. Shell context: `ShellContext::load()` (`src/llm/shell_context.rs:57`, self-contained — no manager
   needed), appended via `format_for_system_message()` if `Some`.
4. Attached files: same caps wgpu already enforces (512 KB/file, 1 MB total), same
   `--- File: {name} ---` framing, same truncation notice.

**Not included:** MCP tool specs (stays provider-only, Section 5) and
`agent_action::system_prompt_instructions()` (stays a separate append at each call site, since M5a's
Task 1 already wired that specifically for the direct-provider path's inline-action-confirm flow, and
it is not part of "context" in the sense this builder covers).

**Call-site shapes** (Section 2 of the brainstorm, confirmed):

- Direct-provider: `system_text.push_str(&addendum.text)`.
- ACP: `format!("{}\n\n{user_content}", addendum.text)` — replaces the raw `user_content` passed to
  `try_send_prompt`/`prompt`.

## 4. MCP config loading, consolidated

**New function**, `src/llm/mcp/config.rs`:

```rust
/// Load global config, then merge in project-local `.petruterm/mcp.json` if the cwd is trusted.
/// Consolidates the identical merge logic `construct.rs`/`ui/mod.rs` each currently inline.
pub fn load_merged(cwd: &Path) -> McpConfig
```

Behavior: `load_global()`, then `load_local(cwd)` merged in only if `trust::is_trusted(cwd)` and
`.petruterm/mcp.json` exists — exactly the logic currently duplicated in `construct.rs` (gpui) and
`ui/mod.rs`'s own `UiManager::new` (wgpu). Both binaries' existing `McpManager::start_all(&cfg)` call
sites switch to calling this instead of their own inline merge (pure refactor — same resulting
`McpConfig`, verified by the existing MCP-loading tests continuing to pass unchanged).

**New mapping function**, `src/llm/mcp/config.rs` (or a small new `src/llm/mcp/acp_bridge.rs` if this
grows past a few lines — decided at plan-writing time against the file's real line count):

```rust
/// Map this project's own MCP config shape to the ACP protocol's `McpServer` list.
pub fn to_acp_servers(config: &McpConfig) -> Vec<agent_client_protocol::schema::McpServer>
```

Maps each `(name, McpServerConfig{command, args, env})` entry to
`McpServer::Stdio(McpServerStdio{name, command, args, env: Vec<EnvVariable>, meta: None})` —
`McpServerConfig::env: HashMap<String,String>` converts to `Vec<EnvVariable{name, value, meta: None}>`
entry-by-entry (order doesn't matter — no protocol requirement on it, confirmed by inspecting the
schema crate directly).

## 5. ACP session wiring

**`AcpSession::connect`** (`src/llm/acp/mod.rs:47`) gains one parameter:

```rust
pub async fn connect(
    cfg: &AcpAgentConfig,
    cwd: &Path,
    mcp_servers: Vec<agent_client_protocol::schema::McpServer>,
) -> Result<Self>
```

Threaded down into `run_session` (`src/llm/acp/session.rs:29`, gains the same parameter) and into the
`NewSessionRequest` it builds (`session.rs:296`, currently `NewSessionRequest::new(&cwd)` — becomes
`NewSessionRequest::new(&cwd).mcp_servers(mcp_servers)` or equivalent builder call, exact method name
confirmed against the crate's real `NewSessionRequest` API at plan-writing time).

**Both `spawn_acp_connect` call sites** (`src/app/ui/mod.rs:27`, `src/gpui_shell/chat_panel/
backend.rs:81`) gain the same addition, symmetrically: when `config.llm.enabled`, call
`mcp_config::load_merged(&cwd)` then `mcp_config::to_acp_servers(&loaded)`, pass the result into
`AcpSession::connect`; when disabled, pass an empty `Vec::new()`.

**`AcpSession`/`run_session` gain no dependency on `McpManager`** — they only ever see the already-
mapped `Vec<McpServer>`, keeping the ACP session lifecycle fully independent of the direct-provider
path's tool-proxying machinery, per the confirmed scope boundary.

## 6. Consumer wiring at each call site

**wgpu (`src/app/ui/mod.rs::submit_ai_query`):**
- Direct-provider branch (~lines 856-931): the inline skill/steering/shell-context/attached-files
  block is replaced by one call to `prompt_context::build_prompt_addendum(...)`, appended to
  `system_text` exactly as today. Pure refactor — the resulting `system_text` must be byte-identical
  to before for the same inputs (verified by the shared builder's own unit tests, not by any change in
  this call site's own behavior).
- ACP branch (~lines 803-843): gains a `build_prompt_addendum` call, prepended to `user_content` before
  `try_send_prompt`.

**gpui_shell (`chat_panel/stream.rs::submit`):**
- Direct-provider branch: `system_text` (currently a bare `format!("{}\n\n{}", load_system_prompt(),
  system_prompt_instructions())`, from M5a Task 1) gains the builder's addendum appended between the
  two existing pieces or after them (exact ordering decided at plan-writing time to match wgpu's own
  ordering for consistency between binaries).
- ACP branch: gains a `build_prompt_addendum` call, prepended to the prompt text passed to
  `try_send_prompt`, mirroring wgpu's ACP branch exactly.

## 7. Testing

Logic-only, per this project's established convention:

- `prompt_context::build_prompt_addendum`: skill match found vs. active-skill-continues vs. no match;
  steering block present/absent; attached-file size caps (per-file and total) enforced correctly;
  empty-everything case returns an empty `PromptAddendum` (no stray blank lines/headers). Uses fake
  in-memory `SkillManager`/`SteeringManager` state (both already support constructing from explicit
  data for tests, per their existing test suites) — no live filesystem beyond what those managers'
  own existing tests already use.
- `mcp_config::to_acp_servers`: a `McpConfig` with 1-2 entries maps to the expected `Vec<McpServer>`
  shape, including env-var conversion.
- `mcp_config::load_merged`: covered by the existing global/local-merge tests already in
  `src/llm/mcp/config.rs`, now exercised through the new consolidated function instead of duplicated
  inline logic.
- No test spawns a real MCP server process, a real ACP agent process, or asserts on live network/
  subprocess behavior — those are dogfooded by hand (Section 8).

## 8. Manual testing required (cannot be verified from the agent sandbox)

- With a skill installed and a matching query: both backends' responses show the skill's instructions
  took effect (matches wgpu's already-working direct-provider behavior; new for gpui_shell's
  direct-provider and both binaries' ACP paths).
- With an MCP server configured in `mcp.json`: the ACP agent (e.g. Claude Code's own ACP adapter)
  reports that server's tools as available and can call them, without `McpManager` itself needing to be
  involved (confirm via server logs / agent's own tool-list, not via petruterm's `/mcp` command, which
  only reports `McpManager`'s own connected-server state — the direct-provider path's view).
- Attaching a file via the composer's file picker and asking a question about it: both backends'
  responses reflect the file's content.
- `/skills` and `/mcp` slash-command text in gpui_shell: still out of this milestone's scope (they
  report `McpManager`/`SkillManager`'s own state text, a separate small follow-up — the actual context
  *injection* this milestone wires does not depend on those commands reporting correctly).

## 9. Deferred (explicitly out of scope)

- Fixing `/skills`/`/mcp` slash-command text in gpui_shell to report real state instead of "not wired"
  — cosmetic, does not block context injection actually working, separate small follow-up.
- Any change to `McpManager` itself, or to the direct-provider path's tool-calling/proxying — untouched
  per the confirmed scope boundary.
- HTTP/SSE MCP server transports (`McpServer::Http`/`Sse`) — this project's own `mcp.json` config shape
  (`McpServerConfig`) only ever describes stdio servers today; only `McpServer::Stdio` is populated.
