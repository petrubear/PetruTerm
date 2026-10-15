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
    }
end

return module
