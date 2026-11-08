# Contract: output-based state

Applies to every pane that has a detected agent, in the gpui Agents list.

| Aspect | Rule |
|--------|------|
| Signal | the terminal's reader thread stamps `last_agent_output_ms` on a read that arrives more than 300 ms after the last input write |
| Working | last agent output less than 3 s ago |
| Idle | older, and the pane has produced agent output before |
| Running | the pane has never produced agent output (shown as today) |
| Activity text | none (state only) |
| Precedence | a hook-reported state wins; a hook `working` older than 120 s while the observer says idle becomes idle |
| Switch | `agents.output_state = false` disables it (rows show state only from hooks) |
| Cost | two relaxed atomic stores per read/write; the gpui poll reads the atomics only while the Agents list is on screen |
| Typing | keystroke echo within 300 ms of an input write never counts |
| Cards | the resulting state uses the same card styles as hook-reported states |
