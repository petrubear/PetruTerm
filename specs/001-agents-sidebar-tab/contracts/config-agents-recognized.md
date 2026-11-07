# Contract: `agents.recognized` (Lua config)

File: `config/default/notifications.lua` (user override in the user's config dir).

```lua
config.agents = {
    -- existing keys unchanged: notifications, ring, bell_attention
    recognized = {
        -- Appended to the built-in list; an entry with the same name replaces the built-in.
        { name = "Gemini CLI", process = { "gemini" }, color = "#4285f4" },
    },
}
```

Rules
- `name`: non-empty string. `process`: non-empty array of non-empty strings, matched against the process basename, or the script basename when the process is `node`/`bun`/`deno`/`python*`.
- `color` (optional): `#rrggbb` color of the agent's name in the Agents list. An invalid color is ignored (the entry still loads); a replacement entry without `color` keeps the built-in color. Built-in colors: Claude Code orange, Kiro purple, Codex green, Copilot pink, Antigravity blue.
- Invalid entries are skipped with a logged warning; the rest of the config loads normally.
- Absent key = built-in defaults only. Backward compatible: no existing key changes meaning.
- Applied via the existing full config re-evaluation and swap (hot reload).

Built-in defaults (names to verify at implementation): Claude Code (`claude`), Kiro (`kiro-cli`, `kiro`), Codex (`codex`), Copilot (`copilot`), Antigravity (`agy`, `antigravity`).
