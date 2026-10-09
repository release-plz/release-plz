#!/usr/bin/env bash
set -euo pipefail
source /workspace/.devtools/activate.sh
export CARGO_TARGET_DIR=/workspace/release-plz/target
candidate="$1"
case "$candidate" in
  release-order) cd /workspace/perf-worktrees/core-update ;;
  owned-diffs) cd /workspace/perf-worktrees/owned-package-diffs ;;
  *) exit 2 ;;
esac
# Shared-target Cargo fingerprints can otherwise reuse an older worktree's artifact.
rg --files crates -g '*.rs' -g Cargo.toml -0 | xargs -0 touch
touch Cargo.toml
cargo clean --release -p release-plz -p release_plz_core -p cargo_utils -p git_cmd -p next_version -p fake_package -p test_logs
cargo test --release -p release_plz_core --lib --no-run --message-format=json > "/workspace/perf-results/core-update/${candidate}-build.json" 2> "/workspace/perf-results/core-update/${candidate}-build.log"
python3 - "$candidate" <<'PY'
import json, pathlib, shutil, hashlib, sys
candidate=sys.argv[1]
out=pathlib.Path('/workspace/perf-results/core-update')
for line in (out/f'{candidate}-build.json').read_text().splitlines():
    x=json.loads(line)
    if x.get('executable') and x.get('target',{}).get('name')=='release_plz_core':
        assert not x['fresh'], 'stale core artifact reused'
        assert pathlib.Path(x['manifest_path']) == pathlib.Path.cwd()/'crates/release_plz_core/Cargo.toml'
        binary=pathlib.Path(x['executable'])
        copy=out/f'{candidate}-core-tests'
        shutil.copy2(binary,copy)
        hashes=[hashlib.sha256(p.read_bytes()).hexdigest() for p in (binary,copy)]
        assert hashes[0]==hashes[1]
        (out/f'{candidate}-binary.sha256').write_text('\n'.join(f'{h}  {p}' for h,p in zip(hashes,(binary,copy)))+'\n')
        print(f'{candidate} test binary: {copy}; SHA256 {hashes[0]}')
PY
cd crates/release_plz_core
case "$candidate" in
 release-order) filter=perf_bench_release_order ;;
 owned-diffs) filter=perf_bench_fill_commits ;;
esac
"/workspace/perf-results/core-update/${candidate}-core-tests" --list | rg "$filter"
for run in 1 2 3; do
    /workspace/perf-results/core-update/baseline-core-tests "$filter" --ignored --nocapture --test-threads=1 > "/workspace/perf-results/core-update/${candidate}-before-run-${run}.log" 2>&1
    "/workspace/perf-results/core-update/${candidate}-core-tests" "$filter" --ignored --nocapture --test-threads=1 > "/workspace/perf-results/core-update/${candidate}-run-${run}.log" 2>&1
    rg "test result: ok. 1 passed" "/workspace/perf-results/core-update/${candidate}-before-run-${run}.log" "/workspace/perf-results/core-update/${candidate}-run-${run}.log"
done
if [[ "$candidate" == release-order ]]; then
    "/workspace/perf-results/core-update/${candidate}-core-tests" release_order::tests --test-threads=1 > "/workspace/perf-results/core-update/${candidate}-tests.log" 2>&1
else
    "/workspace/perf-results/core-update/${candidate}-core-tests" command::update::updater::tests --test-threads=1 > "/workspace/perf-results/core-update/${candidate}-tests.log" 2>&1
fi
