-- PetruTerm LLM configuration (Phase 2)
-- Set enabled = true and provide your api_key to activate AI features.

local module = {}

function module.apply_to_config(config)
  config.llm = {
    enabled  = false,    -- Set to true to enable AI features

    provider = "openrouter",                               -- "openrouter" | "ollama" | "lmstudio"
    model    = "meta-llama/llama-3.1-8b-instruct:free",   -- Free model for testing
    api_key  = os.getenv("OPENROUTER_API_KEY"),            -- Or paste key directly (not recommended)
    base_url = nil,                                        -- nil = use provider default

    -- Local provider examples (no api_key needed):
    -- provider = "ollama",   model = "llama3.2"   -- base_url defaults to http://localhost:11434/v1
    -- provider = "lmstudio", model = "..."         -- base_url defaults to http://localhost:1234/v1

    features = {
      nl_to_command  = true,   -- Natural language → shell command (Ctrl+Space)
      explain_output = true,   -- Explain selected/last output
      fix_last_error = true,   -- Fix suggestion on non-zero exit
      context_chat   = true,   -- Multi-turn chat with terminal context
    },

    -- Number of terminal output lines sent as context with each query.
    context_lines = 50,

    -- ── Chat panel appearance ───────────────────────────────────────────────
    ui = {
      -- Panel width in terminal columns.
      width_cols   = 55,
      -- Panel background color (RGBA hex or table {r,g,b,a}).
      background   = "#1a1726",
      -- Message colors.
      user_fg      = "#bfe5ff",   -- User message text
      assistant_fg = "#8dff85",   -- Assistant message text
      input_fg     = "#ffffff",   -- Input line text
    },
  }
end

return module
