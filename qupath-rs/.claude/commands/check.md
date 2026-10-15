Run the project's quality gates and report failures:

1. `cargo fmt --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test --release`
4. `scripts/check-sizes.sh` (no file over 300 lines, no function over 50 lines)
