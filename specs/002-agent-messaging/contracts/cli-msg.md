# Contract: `petruterm msg`

```
petruterm msg [--no-reply] <name> <text...>
petruterm msg [--no-reply] <name> -        # read the text from stdin
```

- Runs as an ordinary shell command inside a PetruTerm pane (found through the `petruterm` shim in the pane's PATH).
- `<text...>` is joined with single spaces; use `-` to pass text with quotes or newlines safely through a heredoc.
- `--no-reply`: the delivered message states that no answer is expected.
- The caller's identity is never an argument; the app derives it from the connection (research R4).

Output: one line on stdout, and an exit code.

| Result | stdout | Exit |
|---|---|---|
| Delivered | `delivered to <name>` | 0 |
| Usage error | usage text on stderr | 2 |
| Unknown name | `error: unknown agent <name>` | 3 |
| Not accepting | `error: <name> is not accepting messages (turn on "Accept messages" in the Agents list)` | 4 |
| Rate limited | `error: rate limited, try again in <n>s` | 5 |
| Conversation limit | `error: conversation limit reached (10 messages); wait 2 minutes or ask the user` | 6 |
| Too long | `error: message too long (max 4000 characters)` | 7 |
| Agent gone | `error: <name> is no longer running` | 8 |
| Sender unknown / outside PetruTerm | `error: cannot send from here (run this inside a PetruTerm pane)` | 9 |
| Message to self | `error: cannot message yourself` | 10 |
| Empty | `error: empty message` | 11 |
| Unsupported build | `error: messaging is not available in this build` | 12 |
| App unreachable (no socket, timeout) | `error: PetruTerm did not answer` | 13 |

Outside PetruTerm (no `PETRUTERM_*` variables) the command prints the code-9 line and exits 9; it never contacts any other process. Hooks and `notify` keep their exit-0-when-outside behavior; `msg` is explicit because the caller wants to know.
