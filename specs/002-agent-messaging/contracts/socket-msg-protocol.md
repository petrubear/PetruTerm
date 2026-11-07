# Contract: message request/response on the control socket

Same socket and newline-delimited JSON as hooks (`src/ipc`). A request line carries a `msg` object; hook messages without it are unchanged.

Request (client to app):
```json
{ "pane": 7, "msg": { "to": "kterm", "text": "What is 7*6?", "no_reply": false } }
```
Response (app to client), one line on the same connection, then the connection closes:
```json
{ "result": "delivered", "detail": "kterm" }
{ "result": "rate_limited", "retry_after_s": 12 }
{ "result": "unknown_name", "detail": "kterm" }
```
`result` is one of: `delivered`, `unknown_name`, `not_accepting`, `rate_limited`, `conversation_limit`, `too_long`, `agent_gone`, `sender_unknown`, `self_message`, `empty`, `unsupported`.

Rules
- The server fills `peer_pid` itself from the connection; any such field in the request is ignored.
- The app replies within 2 s or the server answers `{ "result": "timeout" }` (the CLI maps it to exit 13).
- Lines are capped at the existing 16 KiB; text over 4000 characters after sanitizing is refused as `too_long`.
- Only one `msg` per connection is answered; extra lines are ignored.
- Builds without messaging answer `unsupported` immediately.
- Compatible with older clients: a request without `msg` behaves exactly as before.
