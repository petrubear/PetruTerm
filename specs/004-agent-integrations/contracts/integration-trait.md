# Contract: the AgentIntegration interface

Where: `src/ipc/integrations/mod.rs` (trait, registry, helpers); one file per agent next to it.

## Adding an agent (the recipe)

1. Create `src/ipc/integrations/<agent>.rs` with a unit struct implementing `AgentIntegration`: at minimum `id()`, `events()` and `on_hook()`.
2. Add one line to the registry slice in `mod.rs`.
3. Add fixture payloads under `tests/fixtures/agents/<agent>/<Event>.json` and one `contract_case!` line per event in the integration's test module.
4. If the agent has a hook file, implement `binaries()`, `hook_file()`, `install()`, `remove()`.
5. Make sure the agent is in `default_agents()` (or the user's `agents.recognized`) so its pane is detected.

Nothing else changes: the tracker, rows, cards, socket and sidebar are agent-neutral.

## Behavior every implementation must satisfy (enforced by the shared contract test)

- `on_hook` never panics on any JSON (null, wrong types, huge strings) and returns `None` for unknown events.
- A returned report has `pane` set, a state from `working|needs-input|done|idle` or none (session-only), activity already cleaned with `clean_activity`, no control characters.
- Activity is set only with `working`.
- Reading the payload never allocates unboundedly: it works on the `Value` already read under the 64 KB limit.
- `hook_stdout(event)` is `Some` exactly for the events where the agent requires output, and that text is valid JSON.
- Prompts, commands and paths are never logged.

## Registry

```
find(id) -> Option<&'static dyn AgentIntegration>
all()    -> &'static [&'static dyn AgentIntegration]
```
Unknown `id` is not an error for the CLI (see `hook-cli.md`).
