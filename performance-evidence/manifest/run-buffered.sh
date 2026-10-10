#!/usr/bin/env bash
set -euo pipefail
source /workspace/.devtools/activate.sh
export CARGO_TARGET_DIR=/workspace/release-plz/target
cd /workspace/perf-manifest
cp /workspace/perf-evidence/manifest/workspace_members.before.rs crates/cargo_utils/src/workspace_members.rs
cp /workspace/perf-evidence/manifest/cargo.before.rs crates/cargo_utils/src/cargo.rs
cp /workspace/perf-evidence/manifest/local_manifest.before.rs crates/cargo_utils/src/local_manifest.rs
cargo bench --locked -p cargo_utils --bench manifest_perf -- dependency_tables_mut --save-baseline tables-main2 > /workspace/perf-evidence/manifest/tables-baseline2.log 2>&1
cp /workspace/perf-evidence/manifest/local_manifest.buffered.rs crates/cargo_utils/src/local_manifest.rs
cargo bench --locked -p cargo_utils --bench manifest_perf -- dependency_tables_mut --baseline tables-main2 > /workspace/perf-evidence/manifest/tables-buffered.log 2>&1
cp /workspace/perf-evidence/manifest/cargo.after.rs crates/cargo_utils/src/cargo.rs
