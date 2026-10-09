#!/usr/bin/env bash
set -euo pipefail
source /workspace/.devtools/activate.sh
export CARGO_TARGET_DIR=/workspace/release-plz/target
cd /workspace/perf-worktrees/skip-unused-commit-metadata
cargo clean --release -p release-plz -p release_plz_core -p cargo_utils -p git_cmd -p next_version -p fake_package -p test_logs
python3 - <<'PY'
from pathlib import Path
for source in Path('crates').rglob('*.rs'):
    source.touch()
PY
cargo test --release -p release-plz --no-default-features --test all update_changelog_includes_distinct_author_and_committer -- --nocapture > /workspace/perf-results/skip-unused-commit-metadata/integration-test.log 2>&1
cargo test --release -p release_plz_core --lib --no-run --message-format=json > /workspace/perf-results/skip-unused-commit-metadata/after-build.json 2> /workspace/perf-results/skip-unused-commit-metadata/after-build.log
python3 - <<'PY'
import json, pathlib, shutil, hashlib
out = pathlib.Path('/workspace/perf-results/skip-unused-commit-metadata')
for line in (out / 'after-build.json').read_text().splitlines():
    result = json.loads(line)
    if result.get('executable') and result.get('target', {}).get('name') == 'release_plz_core':
        assert result['fresh'] is False, result
        assert result['target']['src_path'].startswith('/workspace/perf-worktrees/skip-unused-commit-metadata/'), result
        binary = pathlib.Path(result['executable'])
        copied = out / 'after-core-tests'
        shutil.copy2(binary, copied)
        (out / 'after-binary.sha256').write_text(f'{hashlib.sha256(copied.read_bytes()).hexdigest()}  {copied}\n')
PY
for run in 1 2 3; do
    /workspace/perf-results/core-update/baseline-core-tests perf_bench_fill_commits --ignored --nocapture --test-threads=1 > "/workspace/perf-results/skip-unused-commit-metadata/before-run-${run}.log" 2>&1
    /workspace/perf-results/skip-unused-commit-metadata/after-core-tests perf_bench_fill_commits --ignored --nocapture --test-threads=1 > "/workspace/perf-results/skip-unused-commit-metadata/after-run-${run}.log" 2>&1
done
python3 - <<'PY'
from pathlib import Path
for phase in ['before', 'after']:
    for run in range(1, 4):
        text = Path(f'/workspace/perf-results/skip-unused-commit-metadata/{phase}-run-{run}.log').read_text()
        assert text.count('FILL_COMMITS ') == 4, text
        assert '1 passed' in text, text
PY
