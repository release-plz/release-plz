#!/usr/bin/env bash
set -euo pipefail

# Usage: bash reproduce.sh <main checkout> <patched checkout> <output directory>
# Run exclusively: no other builds or CPU-heavy benchmarks during this script.
perf_source=$(cd -- "$(dirname -- "$0")" && pwd)
perf_before=$1
perf_after=$2
perf_output=$3
mkdir -p "$perf_output"
for perf_side in before after; do
    if [ "$perf_side" = before ]; then perf_repo=$perf_before; else perf_repo=$perf_after; fi
    perf_example="perf_root_manifest_$perf_side"
    mkdir -p "$perf_repo/crates/release_plz/examples"
    cp "$perf_source/perf_root_manifest.rs" "$perf_repo/crates/release_plz/examples/$perf_example.rs"
    (
        cd "$perf_repo"
        cargo clean --release -p cargo_utils -p release_plz_core -p release-plz
        cargo build --locked --release -p release-plz --example "$perf_example"
        cp "${CARGO_TARGET_DIR:-target}/release/examples/$perf_example" "$perf_output/$perf_side"
    )
done
sha256sum "$perf_output/before" "$perf_output/after" | tee "$perf_output/binary-hashes.txt"
if cmp -s "$perf_output/before" "$perf_output/after"; then
    echo 'Expected distinct baseline and candidate binaries' >&2
    exit 1
fi
for perf_run in 1 2 3; do
    "$perf_output/before" | tee "$perf_output/before-$perf_run.txt"
    "$perf_output/after" | tee "$perf_output/after-$perf_run.txt"
done
