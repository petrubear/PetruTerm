# Contract: hook files we write

All commands carry the marker `PETRUTERM_BIN`, use `timeout` 5 (seconds), and are no-ops outside PetruTerm. `<Event>` is the event name.

## Kiro: `~/.kiro/hooks/petruterm.json` (wholly ours; `$KIRO_HOME` honored)

```json
{
  "version": "v1",
  "hooks": [
    { "name": "petruterm-UserPromptSubmit", "trigger": "UserPromptSubmit",
      "action": { "type": "command",
                  "command": "[ -z \"$PETRUTERM_BIN\" ] || \"$PETRUTERM_BIN\" hook kiro UserPromptSubmit" },
      "timeout": 5 }
  ]
}
```
One entry for each of `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `Stop`, `SessionEnd`. No `matcher` (all tools). The command prints nothing.

## Codex: `~/.codex/hooks.json` (merged)

```json
{ "hooks": {
    "PreToolUse": [ { "matcher": "*", "hooks": [
        { "type": "command", "timeout": 5,
          "command": "[ -z \"$PETRUTERM_BIN\" ] || \"$PETRUTERM_BIN\" hook codex PreToolUse" } ] } ],
    "UserPromptSubmit": [ { "hooks": [ { "type": "command", "timeout": 5, "command": "..." } ] } ]
} }
```
Events: `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PermissionRequest`, `Stop`, `Interrupt`, `SessionEnd`. Matcher only on tool events. The wrapper key `"hooks"` is **UNVERIFIED** (see research R2); it is a single constant and a fixture.

## Antigravity: `~/.gemini/config/hooks.json` (merged under the hook name `petruterm`)

```json
{ "petruterm": {
    "SessionStart":  [ { "type": "command", "timeout": 5, "command": "<agy command SessionStart>" } ],
    "PreInvocation": [ { "type": "command", "timeout": 5, "command": "<agy command PreInvocation>" } ],
    "PreToolUse":    [ { "matcher": "*", "hooks": [ { "type": "command", "timeout": 5, "command": "<agy command PreToolUse>" } ] } ],
    "PostToolUse":   [ { "matcher": "*", "hooks": [ { "type": "command", "timeout": 5, "command": "<agy command PostToolUse>" } ] } ],
    "Stop":          [ { "type": "command", "timeout": 5, "command": "<agy command Stop>" } ]
} }
```
`<agy command E>` is `if [ -n "$PETRUTERM_BIN" ]; then "$PETRUTERM_BIN" hook agy E; else echo '<default JSON for E>'; fi` where the default JSON is `{"decision":"allow"}` for `PreToolUse` and `{}` for every other event.

## Rules for all three

- An entry is ours iff its command contains `PETRUTERM_BIN`.
- Re-running setup changes nothing (byte-identical output after the first write).
- Removal restores the user's file to its pre-setup content (apart from JSON formatting normalization, documented), verified on sample files with user hooks present.
