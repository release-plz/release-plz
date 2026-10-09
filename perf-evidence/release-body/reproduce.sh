#!/usr/bin/env bash
# Usage: reproduce.sh BASE_CHECKOUT CHANGED_CHECKOUT OUTPUT_DIRECTORY
# Run under a machine-wide lock with no other CPU-heavy work.
set -euo pipefail
base_checkout=$(realpath "$1")
changed_checkout=$(realpath "$2")
mkdir -p "$3"
output_directory=$(realpath "$3")
evidence_directory=$(cd "$(dirname "$0")" && pwd)
example_name=perf_release_body
for variant in before after; do
    checkout=$base_checkout
    if [[ $variant == after ]]; then checkout=$changed_checkout; fi
    example_directory="$checkout/crates/release_plz_core/examples"
    mkdir -p "$example_directory"
    cp "$evidence_directory/$example_name.rs" "$example_directory/$example_name.rs"
    # Worktrees share crate IDs in a shared target directory. Force Cargo to see
    # every source checkout rather than reusing a newer binary from the other tree.
    (cd "$checkout" && python3 - <<'PY_TOUCH'
from pathlib import Path
for source in Path('crates').glob('*/src/**/*.rs'):
    source.touch()
PY_TOUCH
    )
    (cd "$checkout" && cargo clean --release -p release_plz_core -p cargo_utils -p git_cmd -p next_version -p test_logs -p fake_package)
    (cd "$checkout" && cargo build --release -p release_plz_core --example "$example_name")
    target_directory=${CARGO_TARGET_DIR:-"$checkout/target"}
    cp "$target_directory/release/examples/$example_name" "$output_directory/$variant"
    rm "$example_directory/$example_name.rs"
done
sha256sum "$output_directory/before" "$output_directory/after" > "$output_directory/binary-sha256.txt"
for run in 1 2 3; do
    "$output_directory/before" > "$output_directory/run-$run-before.txt"
    "$output_directory/after" > "$output_directory/run-$run-after.txt"
done
