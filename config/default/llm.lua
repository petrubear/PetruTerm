-- PetruTerm LLM configuration
-- AI features are on by default via the ACP agent backend (Claude Code).
-- Set enabled = false to turn them off.

local module = {}

function module.apply_to_config(config)
  config.llm = {
    enabled  = true,    -- Set to false to disable AI features

    -- Backend: "provider" (direct LLM API) or "agent" (ACP agent process like Claude Code CLI).
    -- When set to "agent", the `agent` table below is used instead of provider/model.
    backend  = "agent",

    -- ACP agent config (used when backend = "agent"). Requires Node.js/npx installed.
    -- Test the adapter standalone first: `npx -y @agentclientprotocol/claude-agent-acp`
    -- should hang waiting on stdin (Ctrl+C to kill) — if that fails, it's a
    -- Node/network problem, not a PetruTerm one.
    agent = {
      command      = "npx",
      args         = { "-y", "@agentclientprotocol/claude-agent-acp" },
      -- Auth: if `claude` (Claude Code CLI) is already logged in via OAuth on
      -- this machine, the SDK reuses those credentials and env can stay empty.
      -- Otherwise set your key here (or export ANTHROPIC_API_KEY in your shell).
      env          = {},          -- e.g. { ANTHROPIC_API_KEY = "sk-ant-..." }
      display_name = nil,         -- override label in chat panel header (nil = command basename)
    },
    -- Fallback package name if the one above fails to resolve via npx:
    -- "@zed-industries/claude-code-acp" (older name, still the hardcoded
    -- default in the agent-client-protocol-tokio crate this project vendors).

    provider = "openrouter",                               -- "openrouter" | "ollama" | "lmstudio" | "copilot"
    model    = "meta-llama/llama-3.1-8b-instruct:free",   -- Free model for testing
    api_key  = os.getenv("OPENROUTER_API_KEY"),            -- Or paste key directly (not recommended)
    base_url = nil,                                        -- nil = use provider default

    -- GitHub Copilot (requires active Copilot subscription):
    -- provider = "copilot",
    -- model    = "gpt-4o",  -- also: gpt-4o-mini, claude-3.5-sonnet, o1-mini
    -- api_key is resolved from `api_key` above, then the Keychain, then the GitHub device-flow login.

    -- Local provider examples (no api_key needed):
    -- provider = "ollama",   model = "llama3.2"   -- base_url defaults to http://localhost:11434/v1
    -- provider = "lmstudio", model = "..."         -- base_url defaults to http://localhost:1234/v1

    -- ── Chat panel appearance ───────────────────────────────────────────────
    ui = {
      -- Panel width in terminal columns.
      width_cols   = 55,
      -- Colors are derived from the active theme automatically.
    },
  }
end

return module
