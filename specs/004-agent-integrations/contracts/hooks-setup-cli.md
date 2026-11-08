# Contract: `petruterm hooks setup | remove | status`

```
petruterm hooks setup  [kiro|codex|agy|all] [--dry-run]
petruterm hooks remove [kiro|codex|agy|all] [--dry-run]
petruterm hooks status
petruterm hooks setup claude [--dry-run] [--settings PATH]     # unchanged
```

`all` is the default. Claude is not part of `all`: its shim needs no install.

## setup

- For each selected agent: skip with a note when none of its `binaries()` is on PATH; otherwise merge our entries into its hook file (see `agent-hook-files.md`).
- Merge rules: user entries untouched; ours added once (marker `PETRUTERM_BIN`); idempotent; creates parent directories; backup `<file>.petruterm-bak` before the first change to an existing file.
- Unparsable or unexpected file: refuse for that agent, print why, leave the file unchanged; other agents continue; exit 1 at the end.
- `--dry-run`: print what would change, write nothing.
- Per-agent notes printed after a successful install: Kiro "requires Kiro CLI V3 mode"; Codex "trust required: run /hooks in Codex once"; agy none.

## remove

Strips only entries carrying the marker. Kiro's `petruterm.json` is deleted if it still carries the marker. A user file left empty by the removal is deleted only if it did not exist before our setup (no backup file next to it). `--dry-run` supported.

## status

One line per agent: `not installed`, `installed (N events)`, or `installed, note: <note>`; plus whether the binary was found. Exit 0.

## Exit codes

0 success (including nothing to do and skipped agents), 1 a file was refused or could not be written, 2 usage error.
