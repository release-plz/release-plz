#!/usr/bin/env python3
"""Summarize three raw runs written by run.py; values are ns per operation."""

import json
from pathlib import Path
import re
import statistics
import sys


root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent
pattern = re.compile(
    r"(inherited_lookup templates=\d+|independent_dependents consumers=\d+) .*?ns_per_iteration=([\d.]+)"
)
measurements = {}
for variant in ["baseline", "indexed", "pathless"]:
    measurements[variant] = {}
    for run in range(1, 4):
        contents = (root / f"{variant}-{run}.log").read_text()
        matches = pattern.findall(contents)
        assert len(matches) == 4, (variant, run, matches)
        for case, value in matches:
            measurements[variant].setdefault(case, []).append(float(value))

summary = {}
for variant, cases in measurements.items():
    summary[variant] = {}
    for case, values in cases.items():
        mean = statistics.mean(values)
        baseline_mean = statistics.mean(measurements["baseline"][case])
        summary[variant][case] = {
            "runs_ns": values,
            "mean_ns": mean,
            "mean_reduction_percent": 100 * (1 - mean / baseline_mean),
        }
print(json.dumps(summary, indent=2))
