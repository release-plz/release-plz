#!/usr/bin/env bash
set -euo pipefail
# Run from a directory containing header-harness/ and regex-harness/.
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$script_dir/target}"
for harness in header regex; do
  cargo test --locked --release --manifest-path "$script_dir/$harness-harness/Cargo.toml"
done
cargo bench --locked --manifest-path "$script_dir/header-harness/Cargo.toml" --bench headers --no-run
cargo bench --locked --manifest-path "$script_dir/regex-harness/Cargo.toml" --bench pr_regex --no-run
for run in 1 2 3; do
  for harness in header regex; do
    bench=headers
    if [[ "$harness" == regex ]]; then bench=pr_regex; fi
    result_dir="$script_dir/results/$harness-run-$run"
    mkdir -p "$result_dir"
    CRITERION_HOME="$result_dir" cargo bench --locked \
      --manifest-path "$script_dir/$harness-harness/Cargo.toml" \
      --bench "$bench" -- --noplot 2>&1 | tee "$result_dir/console.txt"
  done
done
python3 "$script_dir/summarize.py"
