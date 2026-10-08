import json
from pathlib import Path
from statistics import median
root = Path('/workspace/perf-results/manifest/criterion')
rows = {}
for path in root.rglob('estimates.json'):
    run = path.parent.name
    if run not in {f'{kind}-{i}' for kind in ['before', 'after'] for i in [1, 2, 3]}:
        continue
    case = str(path.parent.parent.relative_to(root))
    estimates = json.loads(path.read_text())
    estimator = 'slope' if estimates.get('slope') is not None else 'mean'
    rows.setdefault(case, {})[run] = (estimator, estimates[estimator]['point_estimate'] / 1000)
lines = ['| Benchmark | Estimator | Before runs (µs) | After runs (µs) | Before median (µs) | After median (µs) | Time reduction |', '|---|---|---|---|---:|---:|---:|']
for case, runs in sorted(rows.items()):
    before = [runs[f'before-{i}'][1] for i in [1, 2, 3]]
    after = [runs[f'after-{i}'][1] for i in [1, 2, 3]]
    estimator = ', '.join(sorted({e for e, _ in runs.values()}))
    b, a = median(before), median(after)
    lines.append(f'| {case} | {estimator} | ' + ', '.join(f'{v:.3f}' for v in before) + ' | ' + ', '.join(f'{v:.3f}' for v in after) + f' | {b:.3f} | {a:.3f} | {(1-a/b)*100:.2f}% |')
text = '\n'.join(lines) + '\n'
print(text)
Path('/workspace/perf-results/manifest/summary.md').write_text(text)
