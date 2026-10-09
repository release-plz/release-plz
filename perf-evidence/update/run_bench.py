#!/usr/bin/env python3
"""Run three alternating before/after repetitions of temporary updater benchmarks."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import statistics
import subprocess

p = argparse.ArgumentParser()
p.add_argument('--before', type=Path, required=True)
p.add_argument('--after', type=Path, required=True)
p.add_argument('--before-cwd', type=Path, required=True)
p.add_argument('--after-cwd', type=Path, required=True)
p.add_argument('--filter', required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
result = {'benchmark': a.filter, 'runs': [], 'binaries': {}}
for label, binary in [('before', a.before), ('after', a.after)]:
    result['binaries'][label] = {'path': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
for repetition in range(1, 4):
    run = {'repetition': repetition}
    order = [('before', a.before, a.before_cwd), ('after', a.after, a.after_cwd)]
    if repetition % 2 == 0:
        order.reverse()
    for label, binary, cwd in order:
        out = subprocess.run([str(binary), a.filter, '--ignored', '--nocapture', '--test-threads=1'], cwd=cwd, text=True, capture_output=True, env={**os.environ, 'CARGO_NET_OFFLINE': 'true'})
        (a.output.parent / f'{a.filter}-{label}-{repetition}.txt').write_text(out.stdout + out.stderr)
        if out.returncode:
            raise RuntimeError(out.stdout + out.stderr)
        measurements = {}
        for line in out.stdout.splitlines():
            if 'BENCH ' in line:
                _, tail = line.split('BENCH ', 1)
                name, ns, iterations = tail.split()
                measurements[name] = {'ns': float(ns), 'iterations': int(iterations)}
        if not measurements:
            raise RuntimeError(out.stdout)
        run[label] = measurements
        print(repetition, label, measurements, flush=True)
    result['runs'].append(run)
result['summary'] = {}
for name in result['runs'][0]['before']:
    before = statistics.median(r['before'][name]['ns'] for r in result['runs'])
    after = statistics.median(r['after'][name]['ns'] for r in result['runs'])
    result['summary'][name] = {'before_ns': before, 'after_ns': after, 'reduction_percent': 100*(before-after)/before}
a.output.write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result['summary'], indent=2))
