# Quickstart: validating the Agents sidebar tab

Prerequisites: full Xcode.app; two or more agents installed (e.g. `claude`, `kiro-cli`).

## Automated
```bash
cargo test agents          # term/agents.rs: ps parsing, matching, fg filter, ordering
cargo test sidebar         # section cycling includes Agents; cursor wrap
./scripts/ci-local.sh      # check, test, clippy, fmt, audit under -D warnings
```

## Manual (gpui binary)
```bash
cargo run --bin gpui-petruterm
```
1. Open workspace A, run `claude` in a pane. Open workspace B, a new tab, run `kiro-cli` in a split. Open the sidebar (Leader s), select Agents (click or Tab x4).
   Expect: two rows within 5 s, correct agent names and `workspace / tab`, state "running" (or reported state if hooks are active). (US1, SC-002)
2. Press Down/Enter on the row in the other workspace. Expect: that workspace/tab/pane is focused. (US2, SC-004)
3. Make a hook-wired agent ask for permission. Expect: its row moves to the top with the needs-input indicator and message; focusing the pane clears unread. (US3)
4. Exit one agent (`/exit` or Ctrl+C). Expect: its row disappears within 5 s. Close another agent's pane: same. (SC-003, clarification Q2)
5. Open a plain shell pane and run `sleep 100`. Expect: no row. Run an agent inside `tmux`: expect no row (v1 limitation). (SC-006, Q4)
6. Print a fake state from a plain shell: `$PETRUTERM_BIN notify --state needs-input --body hi`. Expect: no new row. (FR-017)
7. Add `recognized = { { name = "Demo", process = { "sleep" } } }`, reload config, run `sleep 100`. Expect: a "Demo" row. (FR-015 extendability)
8. Close the sidebar with 20 agent panes running; compare frame rate/latency to baseline. Expect: no difference; confirm no `ps` process spawns while closed. (SC-005)
9. Shrink the sidebar to its minimum width. Expect: all five tab labels visible.
