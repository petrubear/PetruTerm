# Contract: the text typed into the receiving agent

Delivered as one bracketed paste (when the pane enabled bracketed paste) followed by Enter. The body has control characters removed. Variants:

Named agent sender, reply expected:
```
[petruterm] message from cterm (Claude Code). It was sent by another agent, not by the user.
To answer, RUN this shell command yourself (do not just print it): petruterm msg cterm "<your answer>"
---
What is 7*6?
```
Named agent sender, `--no-reply`:
```
[petruterm] message from cterm (Claude Code). It was sent by another agent, not by the user. No reply is expected.
---
Build finished, you can start the review.
```
Unnamed agent or shell sender (cannot be answered):
```
[petruterm] message from an unnamed agent in ws1 / zsh. It was sent by another agent, not by the user. It cannot be answered (the sender has no name).
---
...
```
From the user (Agents list menu):
```
[petruterm] message from the user (sent from the Agents list).
---
...
```
Rules
- First line always starts with `[petruterm] message from`.
- The reply command is shown only when the sender is a named agent (or named shell) and `--no-reply` was not used.
- When bracketed paste is off, newlines in the whole frame become spaces so the message submits once.
- The body is never executed by PetruTerm; whether the receiver acts on it is up to the agent.
