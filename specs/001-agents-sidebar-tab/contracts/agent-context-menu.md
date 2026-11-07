# Contract: Agents row context menu and rename editor

Right mouse-down on an agent row opens the existing context menu at the pointer.

| Item | Shown when | Action |
|---|---|---|
| Rename | always | opens the inline editor on that row |
| Copy session id | the row has a session id | copies the full id to the clipboard |
| Reset name | the row has a custom name | removes the custom name |

Rename editor
- Inline on the row, prefilled with the current custom name (empty if none), placeholder "agent name".
- Enter: validate. Valid: save and close. Invalid or duplicate: keep the editor open and show a one-line message under the row (`Empty`, `Max 32 characters`, `Only letters, digits, - _ .`, `Name already used by <where>`).
- Escape or a click outside: cancel, previous name kept.
- The editor closes without saving if the row disappears (agent exited, pane closed).

Row display
- No custom name: `[dot] AgentName(color) [sid8] workspace / tab [#N]`.
- With custom name: `[dot] custom-name(color) AgentName(muted) [sid8] workspace / tab [#N]`.
- `sid8` is the first 8 characters of the session id, muted, omitted when unknown.

Name rules: 1-32 chars of letters, digits, `-`, `_`, `.`; unique case-insensitively among custom names; lives with the agent in its pane; never written to disk.

Lookup API (internal, for the later messaging feature): `terminal_for(name)`, `name_of(terminal_id)`.
