#!/usr/bin/env bash
set -euo pipefail

# CI (.github/workflows/ci.yml) installs `dtolnay/rust-toolchain@stable`, which always
# resolves to whatever is newest-at-runtime -- not a pinned version. A locally installed
# `stable` toolchain drifts behind that over time (rustup doesn't auto-update it), so a new
# clippy lint can start failing on CI while staying invisible here. Update first so this
# script tests against the same "current stable" CI does, not a stale local snapshot of it.
rustup update stable --no-self-update

export RUSTFLAGS="-D warnings"

cargo check --all-features
cargo test --lib
cargo clippy --all-features -- -D warnings
cargo fmt --check

# CI installs cargo-audit fresh every run (ephemeral runner); a persistent dev machine
# doesn't need to reinstall it every time, just make sure it's present at all.
command -v cargo-audit >/dev/null || cargo install cargo-audit --quiet
cargo audit
