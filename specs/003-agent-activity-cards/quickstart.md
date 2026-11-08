# Quickstart: validating activity line and state cards

Prerequisites: full Xcode.app (Metal toolchain), `claude` installed, a build of `gpui-petruterm`. No hook setup: the `claude` shim injects the hooks inside a PetruTerm pane.

## 1. Automated checks

```bash
cargo test activity attention claude_hooks hook_cli
cargo test agent_card
./scripts/ci-local.sh
```

Expected: all pass, including the extraction table, cleaning, tracker lifecycle, config parsing and card style tests.

## 2. Run the app

```bash
cargo run --bin gpui-petruterm
```

Open the sidebar Agents tab (`Leader s`, then the Agents section).

## 3. Activity line (User Story 1)

1. In a pane run `claude` and ask: "run `cargo --version`, then read Cargo.toml".
2. Expected: the row detail line changes through `Prompt: run cargo --version...`, `Bash: cargo --version`, `Read: Cargo.toml`.
3. When Claude finishes: the line becomes `done` with the age, no leftover tool text.
4. Ask for a task that triggers a permission prompt: the row shows `needs input: <notification text>`, not the last tool.

## 4. Cleaning and limits

1. Ask Claude to run a command with a very long argument and one containing a newline.
2. Expected: one line, at most 80 characters, ending in `...`; the card height does not change.

## 5. State colors (User Story 2)

1. Run agents in four panes so they are working, waiting for input, done and idle.
2. Expected: four distinct treatments (blue tint and edge, orange, green, none). Check with the dark theme and with `petru-theme-light`.
3. Move the keyboard cursor over each card: the cursor background shows and the colored edge stays.

## 6. Config and hot reload

1. Set `config.agents.activity = "tool"` and save: rows show `Bash`, `Read` only, without restarting.
2. Set `"off"`: rows look as they did before this feature.
3. Restore `"full"`.

## 7. Other sources (User Story 3) and the interrupt check

```bash
petruterm notify --state working --activity "Indexing repo"
petruterm notify --state idle
```

Run in a pane with a detected agent. Expected: the activity appears, then clears.

Interrupt check (records an open point in research R1): while Claude is working, press Esc. Note whether the card returns to `done`/`idle` or stays on `working`; write the result into `research.md`.

## 8. Regression

- An agent without hooks (for example Kiro) looks exactly as before.
- `petruterm` (wgpu binary) builds and behaves as before.
- Workspace rows and the phone view show no activity text.
