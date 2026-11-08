# Contract: `agents.activity` config key

```lua
config.agents = {
    -- ...
    activity = "full",   -- "full" | "tool" | "off"
}
```

| Value | Agents row shows while the agent is working |
|-------|---------------------------------------------|
| `"full"` (default) | the whole activity, for example `Bash: cargo test` |
| `"tool"` | only the label, for example `Bash` (no commands, paths or prompts) |
| `"off"` | nothing extra; the row looks as before this feature |

Rules
- Missing key = `"full"`. An unknown value falls back to `"full"` and is ignored silently, like the other `agents` keys.
- Applied on config hot reload without a restart (the setting is read when rows are built).
- Commands and prompts can contain secrets. They stay in the local sidebar; use `"tool"` if the screen is shared. The key is documented with that note in `config/default/notifications.lua`.
