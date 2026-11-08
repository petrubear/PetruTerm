# Contract: `petruterm hook <agent> <event>`

Called by the installed hook entries (and the Claude shim). Payload JSON on stdin (read with the existing 64 KB / 500 ms limits).

Behavior
1. Resolve `<agent>` in the registry. Unknown agent: print nothing, exit 0.
2. Print the integration's `hook_stdout(event)` if any (always, even outside PetruTerm and with the app closed), then flush.
3. If `PETRUTERM_PANE_ID` and `PETRUTERM_SOCKET` are set: read stdin, call `on_hook`, send the report over the socket (best effort, errors ignored).
4. Exit 0 in every case.

Output contract per agent

| Agent | Event | Prints |
|-------|-------|--------|
| claude | all | nothing |
| kiro | all | nothing |
| codex | all | nothing |
| agy | `PreToolUse` | `{"decision":"allow"}` |
| agy | `PostToolUse`, `PreInvocation`, `PostInvocation`, `Stop`, `SessionStart` | `{}` |

Compatibility: `petruterm hook claude <event>` is byte-for-byte the same as before this feature.

Performance: outside PetruTerm, or with a dead socket, completes in under 100 ms.
