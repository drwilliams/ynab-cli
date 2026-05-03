#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "Running pre-push checks from $repo_root"

echo
echo "1/3 cargo fmt --all -- --check"
cargo fmt --all -- --check

echo
echo "2/3 cargo clippy --workspace --all-targets -- -D warnings"
cargo clippy --workspace --all-targets -- -D warnings

echo
echo "3/3 cargo test --workspace"
cargo test --workspace

echo
echo "Pre-push checks passed."
