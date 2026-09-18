#!/usr/bin/env bash
set -euo pipefail

export RUSTFLAGS="-D warnings"

cargo check --all-features
cargo test --lib
cargo clippy --all-features -- -D warnings
cargo fmt --check
cargo audit
