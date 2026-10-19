#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

echo '::group::Rust checks'
cargo fmt --check
cargo test --workspace --locked
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo bench -p arrpi-core --locked --no-run
cargo bench -p arrpi-core --locked --bench library -- --sample-count 10 --sample-size 1
rulewright --strict
markupwright --check
git diff --check
echo '::endgroup::'
echo '::group::Minimum supported Rust'
rustup toolchain install 1.95.0 --profile minimal
cargo +1.95.0 check --workspace --locked
echo '::endgroup::'
