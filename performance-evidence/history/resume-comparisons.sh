#!/usr/bin/env bash
set -euo pipefail
source /workspace/.devtools/activate.sh
export CARGO_TARGET_DIR=/workspace/perf-coordinator/target
cd /workspace/perf-history
results=/workspace/perf-results/history

# Retain the original Criterion samples as run 1 and repeat both workloads twice.
for run in 2 3; do
    export CRITERION_HOME="$results/criterion-run-$run"
    for variant in before after; do
        cp "$results/checkout-$variant-release-plz" "$CARGO_TARGET_DIR/release/release-plz"
        if [[ "$variant" == before ]]; then
            mode=--save-baseline
        else
            mode=--baseline
        fi
        "$results/checkout-bench" --bench 'update/(single_noop|workspace_10_noop)$' "$mode" history-before --noplot \
            > "$results/checkout-$variant-run-$run.log" 2>&1
    done
done
unset CRITERION_HOME

env -u CARGO_TARGET_DIR "$results/metadata-before-tests" package_compare::tests --nocapture \
    > "$results/metadata-before-tests-rerun.log" 2>&1
env -u CARGO_TARGET_DIR "$results/metadata-before-tests" package_compare::metadata_benchmark::cached_package_comparison --exact --ignored --nocapture \
    > "$results/metadata-before-bench.log" 2>&1
git restore crates/release_plz_core/src/package_compare.rs
git switch perf/package-file-metadata
cargo clean --release -p release_plz_core
python3 - <<'PY'
from pathlib import Path
path = Path('crates/release_plz_core/src/package_compare.rs')
with path.open('a') as out:
    out.write('\n#[cfg(test)]\n#[path = "/workspace/perf-results/history/metadata_benchmark.rs"]\nmod metadata_benchmark;\n')
PY
cargo test --locked --release -p release_plz_core --lib --no-run --message-format=json \
    > "$results/metadata-after-build.json" 2> "$results/metadata-after-build.log"
python3 - <<'PY'
import json, shutil
from pathlib import Path
root = Path('/workspace/perf-results/history')
for line in (root / 'metadata-after-build.json').read_text().splitlines():
    record = json.loads(line)
    if record.get('reason') == 'compiler-artifact' and record.get('executable'):
        shutil.copy2(record['executable'], root / 'metadata-after-tests')
PY
env -u CARGO_TARGET_DIR "$results/metadata-after-tests" package_compare::tests --nocapture \
    > "$results/metadata-after-tests.log" 2>&1
env -u CARGO_TARGET_DIR "$results/metadata-after-tests" package_compare::metadata_benchmark::cached_package_comparison --exact --ignored --nocapture \
    > "$results/metadata-after-bench.log" 2>&1
git restore crates/release_plz_core/src/package_compare.rs
git diff --check
