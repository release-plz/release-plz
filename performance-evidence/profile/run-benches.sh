#!/usr/bin/env bash
set -euo pipefail
bench_root=$(cd "$(dirname "$0")" && pwd)
bench_target=${CARGO_TARGET_DIR:-"$bench_root/target"}
touch "$bench_root/harness/src/"*.rs "$bench_root/harness/src/bin/"*.rs "$bench_root/harness/Cargo.toml"
cargo build --locked --release --manifest-path "$bench_root/harness/Cargo.toml" --target-dir "$bench_target"
python3 "$bench_root/create-fixtures.py"
mkdir -p "$bench_root/results"
for bench_run in 1 2 3; do
    if [[ "$bench_run" == 2 ]]; then variants=(after before); else variants=(before after); fi
    for variant in "${variants[@]}"; do
        for size in 100 1000; do
            "$bench_target/release/profile-copy-bench" "$bench_root/fixtures/tracked-$size" "$variant" 20 | tee "$bench_root/results/copy-$size-$variant-$bench_run.csv"
        done
        for size in 1 10 100 500; do
            "$bench_target/release/order" "$size" "$variant" | tee "$bench_root/results/order-$size-$variant-$bench_run.csv"
        done
    done
done
if command -v strace >/dev/null; then
    for variant in before after; do
        strace -f -c -e trace=newfstatat,statx -o "$bench_root/results/copy-1000-$variant-syscalls.txt" "$bench_target/release/profile-copy-bench" "$bench_root/fixtures/tracked-1000" "$variant" 1 > "$bench_root/results/copy-1000-$variant-strace.csv"
    done
fi
