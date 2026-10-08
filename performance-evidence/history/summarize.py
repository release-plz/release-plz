import json, statistics
from pathlib import Path
root = Path('/workspace/perf-results/history')
for scenario in ('history_100', 'workspace_10_one_change'):
    values = {}
    for variant in ('before', 'cached-lock', 'readme'):
        values[variant] = [json.loads((root / 'estimates' / f'{variant}-{run}' / f'{scenario}.json').read_text())['mean']['point_estimate'] / 1e6 for run in (1, 2, 3)]
    print(scenario)
    before = statistics.median(values['before'])
    for variant, runs in values.items():
        median = statistics.median(runs)
        print(f'{variant}: runs_ms={runs}, median_ms={median:.3f}, reduction={(before-median)/before*100:.2f}%')
