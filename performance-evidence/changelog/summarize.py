#!/usr/bin/env python3
"""Summarize three Criterion runs using per-run mean point estimates."""
import json
from pathlib import Path
from statistics import mean

root = Path(__file__).resolve().parent
values = {}
for estimate_file in sorted((root / 'results').glob('*-run-*/**/new/estimates.json')):
    run_directory = estimate_file.relative_to(root / 'results').parts[0]
    run = int(run_directory.rsplit('-', 1)[1])
    benchmark = json.loads(estimate_file.with_name('benchmark.json').read_text())['full_id']
    segments = benchmark.split('/')
    variant = next(segment for segment in segments if segment in ('before', 'after'))
    scenario = '/'.join(segment for segment in segments if segment != variant)
    nanoseconds = json.loads(estimate_file.read_text())['mean']['point_estimate']
    values.setdefault(scenario, {'before': {}, 'after': {}})[variant][run] = nanoseconds

summaries = {}
for scenario, variants in sorted(values.items()):
    assert all(set(runs) == {1, 2, 3} for runs in variants.values()), scenario
    before = [variants['before'][run] / 1000 for run in (1, 2, 3)]
    after = [variants['after'][run] / 1000 for run in (1, 2, 3)]
    summary = dict(before_us=before, after_us=after, mean_before_us=mean(before), mean_after_us=mean(after), reduction_percent=100 * (1 - mean(after) / mean(before)))
    summaries[scenario] = summary
    print(f"{scenario}: before {', '.join(f'{v:.3f}' for v in before)} us; after {', '.join(f'{v:.3f}' for v in after)} us; mean {mean(before):.3f} -> {mean(after):.3f} us; reduction {summary['reduction_percent']:.2f}%")
(root / 'summary.json').write_text(json.dumps(summaries, indent=2) + '\n')
