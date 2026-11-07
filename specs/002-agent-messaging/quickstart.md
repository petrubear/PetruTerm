# Quickstart: validating agent-to-agent messaging

Prerequisites: `gpui-petruterm` built, Claude Code and Kiro installed and logged in, spec 001's Agents list working (names, session ids).

## Automated
```bash
cargo test messaging        # sanitize, frame, limits, conversations, outcome codes
cargo test --test msg_cli   # CLI exit codes against a fake socket; setup idempotency
./scripts/ci-local.sh
```

## Manual (gpui binary)
1. Run `claude` in one pane and `kiro-cli chat` in another. In the Agents list rename them `cterm` and `kterm`. (spec 001)
2. In `cterm`, ask Claude to run `petruterm msg kterm "What is 7*6?"`. Expect: `error: kterm is not accepting messages ...` and exit 4; nothing typed into `kterm`. (US3, SC-003)
3. Right-click `kterm`, "Accept messages". The row shows the `msg` badge. Repeat step 2. Expect: `delivered to kterm`, the framed text appears in Kiro as a submitted turn, Kiro answers. (US1, US3)
4. Run `petruterm agents setup`, restart both agents, repeat step 3 with `cterm` also accepting. Expect: `kterm` replies by running `petruterm msg cterm ...` and `cterm` reports the answer. Repeat 10 times and count successes. (US2, US6, SC-002)
5. Send to a name that does not exist, to `cterm` from `cterm`, and a 5000-character message. Expect exit codes 3, 10, 7 and nothing typed. (SC-003)
6. Send a message containing `\x1b[201~` and other escape bytes: `printf 'a\033[201~b' | petruterm msg kterm -`. Expect the target to receive the text without the escape. (SC-004)
7. Make the two agents answer each other with no end. Expect refusal after 10 messages with the conversation-limit text; send a message from the Agents list menu and confirm a new conversation starts. Burst 11 messages from one sender in a minute: the 11th is rate limited. (US5, SC-005)
8. Right-click an accepting agent, "Send message…", type, Enter. Expect the user-framed text delivered even with acceptance off. Empty and over-long input show a reason and keep the editor open. (US4, SC-006)
9. Exit the target Claude, then send to it. Expect `error: <name> is no longer running` (exit 8) within 3 s. Start a different agent in that pane: acceptance is off again. Quit and reopen the app: nothing accepting, no names. (FR-012, SC-007)
10. Run `petruterm msg kterm hi` from a terminal outside PetruTerm: exit 9, nothing delivered. From another process of the same user that writes a forged `pane` number to the socket: refused as sender unknown. (FR-003)
11. `petruterm agents setup` twice: the second run changes nothing; `petruterm agents remove` removes only the two marked files; an existing different file is backed up. (SC-008)
12. With nothing named or accepting, confirm no `ps` runs while the sidebar is closed. (SC-009)
