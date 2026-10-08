#!/usr/bin/env bash
set -euo pipefail
source /workspace/.devtools/activate.sh
export CARGO_TARGET_DIR=/workspace/perf-target
results=/workspace/perf-results/history
exec 9>/workspace/perf-bench.lock
flock 9
printf 'Acquired benchmark lock at %s\n' "$(date --iso-8601=seconds)" > "$results/progress.log"
test -x /workspace/perf-results/baseline-release-plz
cp /workspace/perf-results/baseline-release-plz "$results/before-release-plz"
for variant in cached-lock readme; do
    if [[ $variant == cached-lock ]]; then
        cd /workspace/perf-history
    else
        cd /workspace/perf-history-readme
    fi
    touch Cargo.toml
    rg --files crates -0 -g '*.rs' -g Cargo.toml | xargs -0 touch
    cargo bench --locked -p release-plz --bench update --no-run > "$results/build-$variant.log" 2>&1
    rg -q 'Compiling release_plz_core' "$results/build-$variant.log"
    cp /workspace/perf-target/release/release-plz "$results/$variant-release-plz"
    python3 - "$results/build-$variant.log" "$results/update-bench" <<'PY'
import pathlib,re,shutil,sys
text=pathlib.Path(sys.argv[1]).read_text()
path=re.findall(r'Executable benches/update.rs \((.*?)\)',text)[-1]
shutil.copy2(path,sys.argv[2])
PY
    cargo test --locked --release -p release-plz --test all update:: > "$results/test-$variant.log" 2>&1
    printf 'Built and tested %s at %s\n' "$variant" "$(date --iso-8601=seconds)" >> "$results/progress.log"
done
cd /workspace/perf-history-readme
for round in 1 2 3; do
    if [[ $round == 2 ]]; then
        order=(readme cached-lock before)
    else
        order=(before cached-lock readme)
    fi
    for variant in "${order[@]}"; do
        cp "$results/$variant-release-plz" /workspace/perf-target/release/release-plz
        "$results/update-bench" --bench 'history_100|workspace_10_one_change' --noplot > "$results/$variant-$round.log" 2>&1
        mkdir -p "$results/estimates/$variant-$round"
        cp /workspace/perf-target/criterion/update/history_100/new/estimates.json "$results/estimates/$variant-$round/history_100.json"
        cp /workspace/perf-target/criterion/update/workspace_10_one_change/new/estimates.json "$results/estimates/$variant-$round/workspace_10_one_change.json"
        printf 'Measured %s round %s at %s\n' "$variant" "$round" "$(date --iso-8601=seconds)" >> "$results/progress.log"
    done
done
cp "$results/readme-release-plz" /workspace/perf-target/release/release-plz
