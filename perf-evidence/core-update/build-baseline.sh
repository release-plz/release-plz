#!/usr/bin/env bash
set -euo pipefail
source /workspace/.devtools/activate.sh
export CARGO_TARGET_DIR=/workspace/release-plz/target
cd /workspace/perf-worktrees/core-update
# Shared-target Cargo fingerprints can otherwise reuse an older worktree's artifact.
rg --files crates -g '*.rs' -g Cargo.toml -0 | xargs -0 touch
touch Cargo.toml
cargo clean --release -p release-plz -p release_plz_core -p cargo_utils -p git_cmd -p next_version -p fake_package -p test_logs
cargo test --release -p release_plz_core --lib --no-run --message-format=json > /workspace/perf-results/core-update/baseline-build.json 2> /workspace/perf-results/core-update/baseline-build.log
python3 - <<'PY'
import json, pathlib, shutil, hashlib
out=pathlib.Path('/workspace/perf-results/core-update')
for line in (out/'baseline-build.json').read_text().splitlines():
    x=json.loads(line)
    if x.get('executable') and x.get('target',{}).get('name')=='release_plz_core':
        assert not x['fresh'], 'stale core artifact reused'
        assert pathlib.Path(x['manifest_path']) == pathlib.Path.cwd()/'crates/release_plz_core/Cargo.toml'
        binary=pathlib.Path(x['executable'])
        copy=out/'baseline-core-tests'
        shutil.copy2(binary,copy)
        hashes=[hashlib.sha256(p.read_bytes()).hexdigest() for p in (binary,copy)]
        assert hashes[0]==hashes[1]
        (out/'baseline-binary.sha256').write_text('\n'.join(f'{h}  {p}' for h,p in zip(hashes,(binary,copy)))+'\n')
        print(f'Baseline test binary: {copy}; SHA256 {hashes[0]}')
PY
cd crates/release_plz_core
/workspace/perf-results/core-update/baseline-core-tests --list | rg '^release_order::perf_bench_release_order::benchmark_release_order: test$'
/workspace/perf-results/core-update/baseline-core-tests --list | rg '^command::update::updater::perf_bench_fill_commits::benchmark_fill_commits: test$'
for run in 1 2 3; do
    /workspace/perf-results/core-update/baseline-core-tests perf_bench --ignored --nocapture --test-threads=1 > "/workspace/perf-results/core-update/baseline-run-${run}.log" 2>&1
    rg "test result: ok. 2 passed" "/workspace/perf-results/core-update/baseline-run-${run}.log"
done
