#!/usr/bin/env bash
set -euo pipefail
cd "${1:-/workspace/perf-worktrees/manifest-scan}"
cargo clean --release -p cargo_utils -p release_plz_core
cargo test --release --locked -p cargo_utils --lib
cargo test --release --locked -p release_plz_core --lib versionless_and_workspace_dependencies_to_update
cargo fmt --all --check
