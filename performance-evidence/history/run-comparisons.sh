#!/usr/bin/env bash
set -euo pipefail
source /workspace/.devtools/activate.sh
export CARGO_TARGET_DIR=/workspace/perf-coordinator/target
cd /workspace/perf-history
results=/workspace/perf-results/history
cp /workspace/perf-coordinator/Cargo.lock Cargo.lock

git switch --detach 74b20485
cargo clean --release -p release_plz_core
cargo bench --locked -p release-plz --no-default-features --features all-static --bench update -- \
    'update/(single_noop|workspace_10_noop)$' --save-baseline history-before --noplot \
    > "$results/checkout-before.log" 2>&1
cp "$CARGO_TARGET_DIR/release/release-plz" "$results/checkout-before-release-plz"
GIT_TRACE2_EVENT="$results/checkout-before-git-trace.jsonl" \
    cargo bench --locked -p release-plz --no-default-features --features all-static --bench update -- \
    update/workspace_10_noop --test > "$results/checkout-before-trace.log" 2>&1
git switch perf/reuse-head-snapshot
cargo bench --locked -p release-plz --no-default-features --features all-static --bench update -- \
    'update/(single_noop|workspace_10_noop)$' --baseline history-before --noplot \
    > "$results/checkout-after.log" 2>&1
cp "$CARGO_TARGET_DIR/release/release-plz" "$results/checkout-after-release-plz"
GIT_TRACE2_EVENT="$results/checkout-after-git-trace.jsonl" \
    cargo bench --locked -p release-plz --no-default-features --features all-static --bench update -- \
    update/workspace_10_noop --test > "$results/checkout-after-trace.log" 2>&1
cargo test --locked --release -p release-plz --no-default-features --features all-static --test all update:: \
    > "$results/checkout-tests.log" 2>&1
mkdir -p "$results/criterion"
cp -r "$CARGO_TARGET_DIR/criterion/update/single_noop" "$results/criterion/"
cp -r "$CARGO_TARGET_DIR/criterion/update/workspace_10_noop" "$results/criterion/"

for variant in before after; do
    if [[ "$variant" == before ]]; then
        git switch --detach 74b20485
    else
        git switch perf/package-file-metadata
    fi
    cargo clean --release -p release_plz_core
    python3 - <<'PY'
from pathlib import Path
path = Path('crates/release_plz_core/src/package_compare.rs')
with path.open('a') as out:
    out.write('\n#[cfg(test)]\n#[path = "/workspace/perf-results/history/metadata_benchmark.rs"]\nmod metadata_benchmark;\n')
PY
    cargo test --locked --release -p release_plz_core --lib --no-run --message-format=json \
        > "$results/metadata-$variant-build.json" 2> "$results/metadata-$variant-build.log"
    python3 - "$variant" <<'PY'
import json, shutil, sys
from pathlib import Path
variant = sys.argv[1]
root = Path('/workspace/perf-results/history')
for line in (root / f'metadata-{variant}-build.json').read_text().splitlines():
    record = json.loads(line)
    if record.get('reason') == 'compiler-artifact' and record.get('executable'):
        shutil.copy2(record['executable'], root / f'metadata-{variant}-tests')
PY
    env -u CARGO_TARGET_DIR "$results/metadata-$variant-tests" package_compare::tests --nocapture \
        > "$results/metadata-$variant-tests.log" 2>&1
    env -u CARGO_TARGET_DIR "$results/metadata-$variant-tests" package_compare::metadata_benchmark::cached_package_comparison --exact --ignored --nocapture \
        > "$results/metadata-$variant-bench.log" 2>&1
    git restore crates/release_plz_core/src/package_compare.rs
done
git diff --check
