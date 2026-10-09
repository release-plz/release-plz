#!/usr/bin/env bash
set -euo pipefail
cd "${1:-/workspace/perf-worktrees/unique-root-manifest}"
cargo clean --release -p cargo_utils -p release_plz_core -p release-plz
cargo test --release --locked -p release-plz --no-default-features --features all-static --test all set_version::
cargo fmt --all --check
