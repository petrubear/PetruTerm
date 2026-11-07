# Contract: Agents list additions (messaging)

Right-click menu (extends spec 001's menu), in this order:

| Item | Shown when | Action |
|---|---|---|
| Rename | always | (spec 001) |
| Accept messages / Stop accepting messages | always | toggles acceptance for that agent |
| Send message… | always | opens the compose editor on the row |
| Copy session id | a session id is known | (spec 001) |
| Reset name | a custom name is set | (spec 001) |

Row: an agent that is accepting shows a small `msg` badge after its name (muted when idle, accent while a message was delivered in the last 10 s).

Compose editor (inline, like the rename editor)
- Placeholder "message to <name or agent>". Enter sends; Escape or a click outside cancels.
- Sends as the user: delivered even if the agent is not accepting; resets the pair's conversation.
- Refusals keep the editor open with a one-line reason: empty, too long (max 4000 characters), agent gone.
- Closes without sending when the agent exits or its pane closes.
- A successful send closes the editor and shows the row with the badge flashing once.
