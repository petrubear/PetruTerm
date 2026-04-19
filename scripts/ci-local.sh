cargo clippy --all-features -- -D warnings
cargo fmt --check
cargo test --lib
cargo audit
