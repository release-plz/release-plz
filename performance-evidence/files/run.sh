#!/usr/bin/env bash
# Run from an otherwise idle machine with cargo, rustc and python3 on PATH.
# Usage: run.sh BASELINE_WORKTREE CHANGED_WORKTREE OUTPUT_DIRECTORY
set -euo pipefail
benchmark_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
baseline_tree=$(cd -- "$1" && pwd)
changed_tree=$(cd -- "$2" && pwd)
mkdir -p -- "$3"
output_dir=$(cd -- "$3" && pwd)
export CARGO_TARGET_DIR="$output_dir/target"

rustc --version > "$output_dir/rustc-version.txt"
git -C "$baseline_tree" rev-parse HEAD > "$output_dir/baseline-commit.txt"
git -C "$changed_tree" rev-parse HEAD > "$output_dir/after-commit.txt"

for variant in baseline after; do
    tree="$baseline_tree"
    if [[ "$variant" == after ]]; then tree="$changed_tree"; fi
    # Shared target directories can otherwise reuse an artifact from a different
    # worktree whose source has the same Cargo package ID and an older mtime.
    touch "$tree/crates/release_plz_core/src/package_compare.rs"
    cargo build --release --locked --manifest-path "$tree/Cargo.toml" \
        -p release_plz_core --message-format=json > "$output_dir/$variant-build.json"
    python3 "$benchmark_dir/build_harness.py" "$CARGO_TARGET_DIR/release/deps" \
        "$output_dir/$variant" "$output_dir/$variant-build.json"
done

sha256sum "$output_dir/baseline" "$output_dir/after" > "$output_dir/binary-sha256.txt"

for run in 1 2 3; do
    if [[ "$run" == 2 ]]; then variants=(after baseline); else variants=(baseline after); fi
    for variant in "${variants[@]}"; do
        "$output_dir/$variant" > "$output_dir/$variant-$run.csv"
    done
done
