# Data Model: Agent-to-Agent Messaging

All state is in memory except the two instruction files written by `petruterm agents setup`. No message text is ever stored.

## MsgRequest (socket, transient)
| Field | Type | Notes |
|---|---|---|
| to | String | target custom name, compared case-insensitively |
| text | String | up to 4000 chars after sanitizing |
| no_reply | bool | the sender says no answer is expected |
| pane_hint | u64 | `PETRUTERM_PANE_ID` of the caller; must agree with the peer pid (R4) |
| peer_pid | u32 | set by the server from the connection, never by the client |

## Outcome (reply to the sender)
`Delivered`, `UnknownName`, `NotAccepting`, `RateLimited { retry_after_s }`, `ConversationLimit`, `TooLong`, `AgentGone`, `SenderUnknown` (not running in a PetruTerm pane or identity mismatch), `SelfMessage`, `Unsupported` (wgpu build / messaging not started), `Empty`. Each has a short text and a distinct exit code (contracts/cli-msg.md).

## Sender (resolved by the router)
| Variant | Shown as | Can be answered |
|---|---|---|
| NamedAgent { name, agent_type } | `name (Claude Code)` | yes, via `name` |
| UnnamedAgent { location } | `an unnamed agent in <workspace / tab>` | no |
| Shell { location, name? } | `a shell in <workspace / tab>` or its custom name | only if named |
| User | `the user (sent from the Agents list)` | no |

## AgentAccept
`terminal_id -> agent pid`. Present = accepting. Dropped when the detection disappears or its pid changes (`retain_live`), at app exit (not persisted), or when the user toggles it off.

## Conversations (limits)
Key: unordered pair of terminal ids. Value: `{ count: u8, last_at: Instant }`. A message more than 2 minutes after `last_at` starts a new conversation (count 1); the 11th within a conversation is refused. A user message to either terminal removes the pair's entry. Entries older than 2 minutes are pruned on access.

## RateLimiter
Key: (sender terminal id, target terminal id). Value: timestamps of accepted messages in the last 60 s (max 10). The refusal carries the seconds until the oldest expires.

## AgentRow additions (spec 001 view model)
`accepting: bool` (shown as a small badge).

## Compose editor state (gpui)
`{ terminal_id, pid, input: TextInput, error: Option<String> }`, pinned to the agent process like the rename editor; closes without sending if the agent is gone.

## Validation rules
- text: control characters removed (C0 incl. ESC and DEL, C1, CR); `\n` kept only when bracketed paste is on; non-empty after trimming; <= 4000 chars after sanitizing.
- name: already validated by spec 001 (`validate_name`).
- limits: 10 messages per sender/target per 60 s; 10 messages per conversation; conversation gap 2 min.

## State transitions
Not accepting -> (user toggles) -> Accepting -> (process gone / replaced / toggle) -> Not accepting. Conversation: new -> counting (1..10) -> limited (refuses) -> reset by gap > 2 min or a user message.
