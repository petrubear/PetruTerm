# Contract: `petruterm notify --activity`

```
petruterm notify [--state S] [--title T] [--body B] [--activity A]
```

- `--activity A` sets the pane's current activity text (see `socket-activity-field.md`). It is meaningful with `--state working`; with any other state it is accepted and ignored by the app.
- Existing behavior is unchanged: exit 0 silently outside a PetruTerm pane, exit 2 for an unknown argument or a missing value, usage text mentions the new flag.
- `--activity` alone (no `--state`, no `--body`) is a valid call only together with `--state working`; otherwise the usage error applies as today.

Examples
```
petruterm notify --state working --activity "Indexing repo"
petruterm notify --state idle
```
