#!/usr/bin/env bash
set -euo pipefail
source /workspace/.devtools/activate.sh
export CARGO_TARGET_DIR=/workspace/release-plz/target
cd /workspace/perf-update
exec 9>/workspace/perf-build.lock
flock 9
EVIDENCE=/workspace/perf-evidence/update
[[ -f "$EVIDENCE/strace-after-100.txt" ]]
cp Cargo.lock "$EVIDENCE/benchmark-Cargo.lock"
cp crates/release_plz/benches/update.rs "$EVIDENCE/update.rs"
cp "$EVIDENCE/original-Cargo.lock" Cargo.lock
cp "$EVIDENCE/original-Cargo.toml" Cargo.toml
cp "$EVIDENCE/original-cli-Cargo.toml" crates/release_plz/Cargo.toml
rm crates/release_plz/benches/update.rs crates/release_plz/benches/inheritance.rs crates/release_plz/benches/inheritance_source.rs
cargo fmt --all
cargo test --locked --no-default-features --features all-static -p release-plz --test all update:: > "$EVIDENCE/tests-update.log" 2>&1
cargo fmt --all -- --check > "$EVIDENCE/fmt.log" 2>&1
git diff --check
