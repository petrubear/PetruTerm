#!/usr/bin/env bash
# Claude Code Stop hook: run ci-local.sh after a block of Rust changes.
# On failure, blocks the stop so Claude sees the output and fixes it.
# Skips when no .rs/Cargo files changed or the tree is identical to the last checked state.
cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0

git status --porcelain | grep -Eq '\.rs$|Cargo\.(toml|lock)$' || exit 0

stamp="$(git rev-parse --git-dir)/claude-ci-stamp"
hash=$({ git diff HEAD; git ls-files -o --exclude-standard -z | xargs -0 cat; } | shasum | cut -d' ' -f1)
[ "$hash" = "$(cat "$stamp" 2>/dev/null)" ] && exit 0
echo "$hash" > "$stamp"

out=$(./scripts/ci-local.sh 2>&1)
[ $? -eq 0 ] && exit 0

jq -n --arg r "scripts/ci-local.sh failed. Fix the errors below, then finish:

$(printf '%s' "$out" | tail -80)" '{decision: "block", reason: $r}'
