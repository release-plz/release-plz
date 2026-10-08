#!/usr/bin/env python3
"""Run three paired Criterion rounds against saved release-plz binaries.

The original #3146 harness embeds target/release/release-plz. Swap only that
executable while retaining the exact same harness for before and after.
Run on a quiet machine without competing compilers or benchmarks.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--before', type=Path, required=True)
parser.add_argument('--after', type=Path, required=True)
parser.add_argument('--harness', type=Path, required=True)
parser.add_argument('--embedded-cli', type=Path, required=True)
parser.add_argument('--benchmark', required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
results = []
for round_number in range(1, 4):
    # Reverse the order in round two to reduce consistent warm-cache bias.
    variants = [('before', args.before), ('after', args.after)]
    if round_number == 2:
        variants.reverse()
    for variant, binary in variants:
        destination = args.output / f'{variant}-{round_number}'
        destination.mkdir(parents=True, exist_ok=True)
        shutil.copy2(binary, args.embedded_cli)
        env = dict(os.environ, CRITERION_HOME=str(destination / 'criterion'))
        command = [str(args.harness), args.benchmark, '--bench', '--noplot']
        with (destination / 'stdout.log').open('w') as stream:
            subprocess.run(command, check=True, stdout=stream, stderr=subprocess.STDOUT, env=env)
        estimates = list((destination / 'criterion').rglob('new/estimates.json'))
        if len(estimates) != 1:
            raise RuntimeError(f'expected one selected benchmark, found {estimates}')
        estimate = json.loads(estimates[0].read_text())
        record = {'round': round_number, 'variant': variant,
                  'mean_ns': estimate['mean']['point_estimate'],
                  'median_ns': estimate['median']['point_estimate']}
        results.append(record)
        (args.output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
        print(json.dumps(record), flush=True)
