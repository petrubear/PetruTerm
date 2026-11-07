# Contract: `session` field on the control-socket message

Existing newline-delimited JSON `ipc::Message` (see `src/ipc/mod.rs`) gains one optional field.

```json
{ "pane": 7, "state": "working", "session": "3f6c0a52-91d4-4b1e-8c3a-0d2f7e5b9a11" }
```

Rules
- `session` is optional. Senders that omit it and receivers that do not know it keep working (unknown/absent fields are already ignored).
- Receiver sanitizes: only `[A-Za-z0-9_-]`, 1-64 chars; anything else is dropped as if absent.
- Valid for any message that carries a `pane`; it does not need `state`. A message with only `pane` and `session` is a session announcement and produces no attention event.
- Set by `petruterm hook claude <event>` from the hook payload's `session_id`. Hook events now include `SessionStart` (mapped to no state change, session only).
- `petruterm notify` gains no flag in this amendment.

Compatibility: no existing field changes meaning; the `petruterm notify` and `hook` CLIs behave as before outside PetruTerm (exit 0 silently).
