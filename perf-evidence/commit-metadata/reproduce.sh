#!/usr/bin/env bash
# Usage: bash reproduce.sh BEFORE_CHECKOUT AFTER_CHECKOUT OUTPUT_DIRECTORY
# The checkouts must not already contain perf_bench_fill_commits.
set -euo pipefail
before_checkout=$(realpath "$1")
after_checkout=$(realpath "$2")
output_directory=$(realpath -m "$3")
script_directory=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
mkdir -p "$output_directory"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$output_directory/target"}
for phase in before after; do
    if [ "$phase" = before ]; then checkout=$before_checkout; else checkout=$after_checkout; fi
    source_file="$checkout/crates/release_plz_core/src/command/update/updater.rs"
    saved_file=$(mktemp)
    cp "$source_file" "$saved_file"
    restore_source() { cp "$saved_file" "$source_file"; rm -f "$saved_file"; }
    trap restore_source EXIT
    python3 - "$source_file" "$script_directory/fill_commits_harness.rs" <<'PY'
import pathlib, sys
target = pathlib.Path(sys.argv[1])
original = target.read_text()
assert 'mod perf_bench_fill_commits' not in original
harness = pathlib.Path(sys.argv[2]).read_text().replace('BENCH_INPUT', '&input')
target.write_text(original + harness)
# A shared target directory can otherwise reuse newer artifacts from another worktree.
for source in target.parents[5].joinpath('crates').rglob('*.rs'):
    source.touch()
PY
    (cd "$checkout" && cargo clean --release -p release-plz -p release_plz_core -p cargo_utils -p git_cmd -p next_version -p fake_package -p test_logs)
    (cd "$checkout" && cargo test --release -p release_plz_core --lib --no-run --message-format=json) > "$output_directory/$phase-build.json" 2> "$output_directory/$phase-build.log"
    python3 - "$output_directory" "$phase" <<'PY'
import hashlib, json, pathlib, shutil, sys
output, phase = pathlib.Path(sys.argv[1]), sys.argv[2]
for line in (output / f'{phase}-build.json').read_text().splitlines():
    item = json.loads(line)
    if item.get('executable') and item.get('target', {}).get('name') == 'release_plz_core':
        assert item['fresh'] is False, item
        binary = output / f'{phase}-core-tests'
        shutil.copy2(item['executable'], binary)
        (output / f'{phase}-binary.sha256').write_text(f'{hashlib.sha256(binary.read_bytes()).hexdigest()}  {binary}\n')
PY
    restore_source
    trap - EXIT
done
for run in 1 2 3; do
    for phase in before after; do
        "$output_directory/$phase-core-tests" perf_bench_fill_commits --ignored --nocapture --test-threads=1 > "$output_directory/$phase-run-$run.log" 2>&1
    done
done
python3 - "$output_directory" <<'PY'
from pathlib import Path
import sys
output = Path(sys.argv[1])
for phase in ['before', 'after']:
    for run in range(1, 4):
        text = (output / f'{phase}-run-{run}.log').read_text()
        assert text.count('FILL_COMMITS ') == 4, text
        assert '1 passed' in text, text
PY
