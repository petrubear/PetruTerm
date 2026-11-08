# Quickstart: validating agent integrations

Prerequisites: full Xcode.app (Metal toolchain), a build of `gpui-petruterm`, and the agents you want to test installed (`kiro-cli` in V3 mode, `codex`, `agy`). Spec 003's activity line and cards must already work with Claude Code.

## 1. Automated checks

```bash
cargo test integrations hooks_setup observer
cargo test --test hook_cli --test hooks_setup
./scripts/ci-local.sh
```

Expected: mapping tests per agent over the fixtures, the shared contract test over all four integrations, file merge/remove tests, the observer tests, and the CLI round trips all pass.

## 2. Install the hooks

```bash
petruterm hooks setup --dry-run
petruterm hooks setup
petruterm hooks status
```

Expected: only agents found on PATH are installed; user entries in `~/.codex/hooks.json` and `~/.gemini/config/hooks.json` are intact; `~/.kiro/hooks/petruterm.json` exists; status prints the Codex trust note. Run `setup` again: "nothing to do".

## 3. Each agent (run in a PetruTerm pane, Agents list open)

For every agent:
1. Start it and submit a prompt that makes it run a command ("run `ls`").
2. Expected: the card turns working with the prompt start, then `Shell: ls`-style activity, then done when it finishes; the state color and edge follow.
3. Check for stray output: the agent's conversation must not contain any text from our hooks (Kiro and Codex inject hook stdout into the model's context).

Agent specifics
- **Kiro**: in V3 mode. Also confirm a `PreToolUse` is not blocked.
- **Codex**: trust the hooks once with `/hooks`; then ask for something that needs approval: the card shows needs input; approve: working; press Esc during a turn: idle.
- **Antigravity**: confirm it does not prompt or change permissions because of our `PreToolUse` reply.

## 4. Record the UNVERIFIED items (update research.md)

| Item | Where to look |
|------|---------------|
| Kiro `UserPromptSubmit` stdin has `prompt` | temporarily point a hook at a script that dumps stdin to a scratch file |
| Kiro `execute_bash` / `fs_*` `tool_input` field names | same |
| Codex `hooks.json` top-level key; other tool names; launch-time hook flag | same, plus `codex --help` |
| Antigravity `terminationReason` values on interrupt; `Stop` firing in the CLI | same |
| Hook processes inherit `PETRUTERM_PANE_ID` | `echo $PETRUTERM_PANE_ID` in a dump script |

Fix any fixture and mapping that differs; if env inheritance fails, implement research R8.

## 5. Output observer

1. With hooks removed (`petruterm hooks remove`), run any agent: the card turns working while it streams and idle about 3 s after it stops.
2. Type in the agent's input box without submitting: the card must not flip to working.
3. Set `config.agents.output_state = false`: cards go back to "running" (hot reload).
4. Start an agent that is not integrated (add it to `agents.recognized`): it gets the same cards from the observer.

## 6. Regression

- Claude Code behaves exactly as in spec 003's quickstart.
- `petruterm` (wgpu binary) builds and runs as before.
- Run each hook command by hand outside PetruTerm (`env -u PETRUTERM_BIN ...`): exit 0, and Antigravity's prints its JSON.
