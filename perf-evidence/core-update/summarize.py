#!/usr/bin/env python3
import json
import re
from pathlib import Path
from statistics import median

out = Path(__file__).resolve().parent
results = {}
for candidate, pattern in [
    ('release-order', r'RELEASE_ORDER scenario=(\w+) packages=(\d+) iterations=(\d+) ns_per_call=([\d.]+)'),
    ('owned-diffs', r'FILL_COMMITS configured=(\w+) commits=(\d+) iterations=(\d+) ns_per_call=([\d.]+)'),
]:
    scenarios = {}
    for side, prefix in [('before', f'{candidate}-before'), ('after', candidate)]:
        for run in (1, 2, 3):
            log = (out / f'{prefix}-run-{run}.log').read_text()
            assert 'test result: ok. 1 passed' in log
            matches = re.findall(pattern, log)
            assert len(matches) == (9 if candidate == 'release-order' else 4)
            for name, size, iterations, value in matches:
                key = f'{name}/{size}'
                item = scenarios.setdefault(key, {'before_ns': [], 'after_ns': [], 'iterations': int(iterations)})
                item[f'{side}_ns'].append(float(value))
    for key, item in scenarios.items():
        before, after = median(item['before_ns']), median(item['after_ns'])
        item['before_median_ns'] = before
        item['after_median_ns'] = after
        item['latency_reduction_percent'] = 100 * (1 - after / before)
        print(candidate, key, json.dumps(item))
    results[candidate] = scenarios
(out / 'summary.json').write_text(json.dumps(results, indent=2)+'\n')
