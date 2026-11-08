local module = {}

function module.apply_to_config(config)
    config.notifications = {
        -- "toast"  — GPU overlay rendered inside the window
        -- "native" — macOS Notification Center (requires notification permission) (default)
        style = "native",
    }

    -- Agent attention: how a pane that needs you (a program that prints an OSC 9/99/777
    -- notification, e.g. a coding agent waiting for input) is surfaced.
    -- Claude Code only emits desktop notifications by itself in Ghostty, Kitty and iTerm2. Here,
    -- either set `"preferredNotifChannel": "terminal_bell"` in ~/.claude/settings.json together
    -- with bell_attention = true below, or add a Claude Code Notification hook that runs
    --   printf '\033]9;Claude Code needs your attention\007' > /dev/tty
    config.agents = {
        notifications  = true,   -- native/toast notification when you are not looking at the pane
        ring           = true,   -- tab dot and pane ring for panes that need attention
        bell_attention = false,  -- also treat BEL as "needs attention" (noisy: vim, tab completion)
        -- What the Agents list shows while an agent works: "full" (tool and target, e.g.
        -- "Bash: cargo test"), "tool" (tool name only) or "off". Commands and prompts can
        -- contain secrets; they are only shown in the local sidebar. Use "tool" when sharing
        -- your screen.
        activity       = "full",
        -- Extra agents for the sidebar "Agents" list, added to the built-ins (Claude Code,
        -- Kiro, Codex, Copilot, Antigravity). `process` matches the program name (or the
        -- script name for node/bun/deno). Same `name` as a built-in replaces it.
        -- `color` ("#rrggbb", optional) colors the agent's name in the list; built-ins have one.
        -- recognized = { { name = "Gemini CLI", process = { "gemini" }, color = "#4285f4" } },
    }
end

return module
