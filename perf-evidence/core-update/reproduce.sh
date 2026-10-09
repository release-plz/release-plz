#!/usr/bin/env bash
# Usage: bash reproduce.sh release-order|owned-diffs BEFORE_WORKTREE AFTER_WORKTREE OUTPUT_DIR
# Worktrees must be disposable clean checkouts of the indicated baseline and PR commit.
# Run this whole script under the benchmark machine's exclusive build/timing lock.
set -euo pipefail
candidate="$1"
before_tree="$(realpath "$2")"
after_tree="$(realpath "$3")"
result_dir="$(realpath -m "$4")"
evidence_dir="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$result_dir"
if [[ -f /workspace/.devtools/activate.sh ]]; then
    source /workspace/.devtools/activate.sh
fi
case "$candidate" in
    release-order) test_filter=perf_bench_release_order ;;
    owned-diffs) test_filter=perf_bench_fill_commits ;;
    *) exit 2 ;;
esac
for side in before after; do
    if [[ "$side" == before ]]; then current_tree="$before_tree"; else current_tree="$after_tree"; fi
    python3 "$evidence_dir/inject-harness.py" "$current_tree" "$candidate" "$side"
    cd "$current_tree"
    # Cargo can reuse stale path dependencies across worktrees in a shared target directory.
    cargo clean --release -p release-plz -p release_plz_core -p cargo_utils -p git_cmd -p next_version -p fake_package -p test_logs
    cargo test --release -p release_plz_core --lib --no-run --message-format=json > "$result_dir/$side-build.json" 2> "$result_dir/$side-build.log"
    python3 - "$result_dir" "$side" <<'PY'
import hashlib, json, pathlib, shutil, sys
out, side = pathlib.Path(sys.argv[1]), sys.argv[2]
for line in (out / f'{side}-build.json').read_text().splitlines():
    item = json.loads(line)
    if item.get('executable') and item.get('target', {}).get('name') == 'release_plz_core':
        assert not item['fresh']
        assert pathlib.Path(item['manifest_path']) == pathlib.Path.cwd()/'crates/release_plz_core/Cargo.toml'
        original = pathlib.Path(item['executable'])
        copied = out / f'{side}-core-tests'
        shutil.copy2(original, copied)
        assert original.read_bytes() == copied.read_bytes()
        (out / f'{side}-binary.sha256').write_text(f'{hashlib.sha256(copied.read_bytes()).hexdigest()}  {copied}\n')
        break
else:
    raise RuntimeError('Missing compiled test executable')
PY
    "$result_dir/$side-core-tests" --list | rg "$test_filter"
done
for run in 1 2 3; do
    for side in before after; do
        if [[ "$side" == before ]]; then current_tree="$before_tree"; else current_tree="$after_tree"; fi
        cd "$current_tree/crates/release_plz_core"
        "$result_dir/$side-core-tests" "$test_filter" --ignored --nocapture --test-threads=1 > "$result_dir/$side-run-$run.log" 2>&1
        rg 'test result: ok. 1 passed' "$result_dir/$side-run-$run.log"
    done
done
