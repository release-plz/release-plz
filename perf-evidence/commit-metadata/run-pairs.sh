#!/usr/bin/env bash
set -euo pipefail
source /workspace/.devtools/activate.sh
cd /workspace/perf-results/skip-unused-commit-metadata
for run in 1 2 3; do
    /workspace/perf-results/core-update/baseline-core-tests perf_bench_fill_commits --ignored --nocapture --test-threads=1 > "before-run-$run.log" 2>&1
    ./after-core-tests perf_bench_fill_commits --ignored --nocapture --test-threads=1 > "after-run-$run.log" 2>&1
done
cp /workspace/perf-results/core-update/baseline-binary.sha256 before-binary.sha256
python3 - <<'PY'
from pathlib import Path
import json, re, statistics
results = {}
for phase in ['before', 'after']:
    results[phase] = {}
    for run in range(1, 4):
        text = Path(f'{phase}-run-{run}.log').read_text()
        rows = re.findall(r'FILL_COMMITS configured=(\w+) commits=(\d+) iterations=(\d+) ns_per_call=([\d.]+)', text)
        assert len(rows) == 4, text
        assert '1 passed' in text, text
        for configured, commits, iterations, nanos in rows:
            key = f'configured={configured} commits={commits}'
            results[phase].setdefault(key, []).append(float(nanos))
summary = []
for key, before in results['before'].items():
    after = results['after'][key]
    before_median = statistics.median(before)
    after_median = statistics.median(after)
    summary.append({'workload': key, 'before_ns': before, 'after_ns': after,
        'before_median_ns': before_median, 'after_median_ns': after_median,
        'reduction_percent': 100 * (1 - after_median / before_median)})
Path('results.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
PY
