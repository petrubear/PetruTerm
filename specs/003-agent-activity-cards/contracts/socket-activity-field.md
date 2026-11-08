# Contract: `activity` field on the control socket

Same socket and newline-delimited JSON as hooks and messaging (`src/ipc`). One optional field is added to the request line.

```json
{ "pane": 7, "state": "working", "title": "Claude Code", "activity": "Bash: cargo test" }
```

Rules
- `activity` is optional. A line without it behaves exactly as before; a line with it is accepted by older apps (unknown fields are ignored).
- The app cleans the value again on receipt: control characters and whitespace runs become one space, trimmed, at most 80 characters (cut with `...`). An empty result counts as absent.
- It is stored only while the state is `working`. A `working` line without `activity` keeps the previous text; any other state clears it.
- It is shown only in the local Agents list. It is never passed to the remote (phone) protocol or to native notifications, and it is not written to disk or logs.
- A state-less line (a plain notification) ignores `activity`.
- The line size limit (16 KiB) is unchanged; an oversized `activity` is truncated by cleaning, it does not drop the state update.
