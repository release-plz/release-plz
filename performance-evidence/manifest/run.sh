#!/usr/bin/env bash
set -euo pipefail
source /workspace/.devtools/activate.sh
export CARGO_TARGET_DIR=/workspace/perf-target
cd /workspace/perf-manifest
exec 9>/workspace/perf-bench.lock
flock 9
results=/workspace/perf-results/manifest
printf 'Acquired benchmark lock at %s\n' "$(date -u --iso-8601=seconds)"
test -x "$results/before"
# Shared target fingerprints otherwise consider another worktree fresh.
cargo metadata --locked --no-deps --format-version 1 > "$results/metadata.json"
mapfile -t workspace_packages < <(python3 -c 'import json; print("\n".join(json.load(open("/workspace/perf-results/manifest/metadata.json"))["workspace_members"]))')
clean_packages=()
for package in "${workspace_packages[@]}"; do
    clean_packages+=(-p "$package")
done
cargo clean --release "${clean_packages[@]}" > "$results/clean.log" 2>&1
cargo fmt --all -- --check > "$results/fmt.log" 2>&1
cargo bench --locked -p release-plz --bench manifest_updates --no-run --message-format=json > "$results/build.jsonl" 2> "$results/build.log"
python3 - <<'PY'
import json, shutil
with open('/workspace/perf-results/manifest/build.jsonl') as f:
    artifacts = [json.loads(line) for line in f if line.startswith('{')]
local_ids = json.load(open('/workspace/perf-results/manifest/metadata.json'))['workspace_members']
for package_id in local_ids:
    builds = [x for x in artifacts if x.get('reason') == 'compiler-artifact' and x.get('package_id') == package_id]
    assert builds and all(not x['fresh'] for x in builds), (package_id, builds)
exe = next(x['executable'] for x in artifacts if x.get('executable') and x.get('target',{}).get('name') == 'manifest_updates')
shutil.copy2(exe, '/workspace/perf-results/manifest/after')
PY
cargo test --release --locked -p release-plz --test all set_version -- --nocapture > "$results/tests.log" 2>&1
for run in 1 2 3; do
    for variant in before after; do
        export CRITERION_HOME="$results/criterion-$variant-$run"
        "$results/$variant" --bench --noplot > "$results/$variant-$run.log" 2>&1
        printf 'Finished %s run %s at %s\n' "$variant" "$run" "$(date -u --iso-8601=seconds)"
    done
done
