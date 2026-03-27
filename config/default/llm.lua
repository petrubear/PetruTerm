-- PetruTerm LLM configuration (Phase 2)
-- Set enabled = true and provide your api_key to activate AI features.

local module = {}

function module.apply_to_config(config)
  config.llm = {
    enabled  = false,    -- Set to true to enable AI features

    provider = "openrouter",                          -- "openrouter" | "ollama" | "lmstudio"
    model    = "meta-llama/llama-3.1-8b-instruct:free",  -- Free model for testing
    api_key  = os.getenv("OPENROUTER_API_KEY"),       -- Or paste key directly (not recommended)
    base_url = nil,                                   -- nil = use provider default

    -- Local provider examples:
    -- provider = "ollama",   base_url = "http://localhost:11434",  model = "llama3"
    -- provider = "lmstudio", base_url = "http://localhost:1234/v1", model = "..."

    features = {
      nl_to_command  = true,   -- Natural language → shell command
      explain_output = true,   -- Explain selected/last output
      fix_last_error = true,   -- Fix suggestion on non-zero exit
      context_chat   = true,   -- Multi-turn chat with terminal context
    },

    context_lines = 50,   -- Lines of terminal output sent as context
  }
end

return module
