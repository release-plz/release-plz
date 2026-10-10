#!/usr/bin/env bash
set -euo pipefail
source /workspace/.devtools/activate.sh
export CARGO_TARGET_DIR=/workspace/release-plz/target
cd /workspace/perf-manifest
cp /workspace/perf-evidence/manifest/workspace_members.before.rs crates/cargo_utils/src/workspace_members.rs
cargo bench --locked -p cargo_utils --bench manifest_perf -- dependency_tables_mut --save-baseline main > /workspace/perf-evidence/manifest/tables-baseline.log 2>&1
cp /workspace/perf-evidence/manifest/local_manifest.after.rs crates/cargo_utils/src/local_manifest.rs
cargo bench --locked -p cargo_utils --bench manifest_perf -- dependency_tables_mut --baseline main > /workspace/perf-evidence/manifest/tables-lazy.log 2>&1
cargo test --locked -p cargo_utils > /workspace/perf-evidence/manifest/tables-tests.log 2>&1
cp /workspace/perf-evidence/manifest/workspace_members.after.rs crates/cargo_utils/src/workspace_members.rs

cp /workspace/perf-evidence/manifest/workspace_members.before.rs crates/cargo_utils/src/workspace_members.rs
cp /workspace/perf-evidence/manifest/local_manifest.before.rs crates/cargo_utils/src/local_manifest.rs
cargo bench --locked -p cargo_utils --bench manifest_perf -- workspace_package --save-baseline main > /workspace/perf-evidence/manifest/lookup-baseline.log 2>&1
cp /workspace/perf-evidence/manifest/cargo.after.rs crates/cargo_utils/src/cargo.rs
cargo bench --locked -p cargo_utils --bench manifest_perf -- workspace_package --baseline main > /workspace/perf-evidence/manifest/lookup-lazy.log 2>&1
cargo test --locked -p cargo_utils > /workspace/perf-evidence/manifest/lookup-tests.log 2>&1
cp /workspace/perf-evidence/manifest/workspace_members.after.rs crates/cargo_utils/src/workspace_members.rs
cp /workspace/perf-evidence/manifest/local_manifest.after.rs crates/cargo_utils/src/local_manifest.rs

cargo bench --locked -p cargo_utils --bench manifest_perf -- workspace_members --save-baseline cached2 > /workspace/perf-evidence/manifest/cached2.log 2>&1
cp /workspace/perf-evidence/manifest/workspace_members.before.rs crates/cargo_utils/src/workspace_members.rs
cargo bench --locked -p cargo_utils --bench manifest_perf -- workspace_members --baseline cached2 > /workspace/perf-evidence/manifest/baseline2.log 2>&1
