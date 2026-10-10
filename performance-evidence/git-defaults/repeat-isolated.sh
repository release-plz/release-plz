#!/usr/bin/env bash
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$script_dir/target}"
mkdir -p logs
mkdir -p binaries
for variant in before after; do
  cp "snapshots/$variant.rs" src/changelog.rs
  cargo bench --locked --bench git_defaults --no-run --message-format=json > "logs/build-$variant.jsonl" 2> "logs/build-$variant.log"
  python3 - "$variant" <<'COPYBIN'
import json, sys, shutil
variant = sys.argv[1]
for line in open(f'logs/build-{variant}.jsonl'):
    m=json.loads(line)
    if m.get('reason') == 'compiler-artifact' and m.get('target', {}).get('kind') == ['bench']:
        shutil.copy2(m['executable'], f'binaries/{variant}')
COPYBIN
done
for round in 2 3; do
  for benchmark in configured_phase empty_phase_control build_generate_custom_1 build_generate_custom_20 build_generate_default_control_1; do
    taskset -c "${BENCHMARK_CPU:-0}" binaries/before --bench "git_defaults/$benchmark" --save-baseline "before-r$round" > "logs/$benchmark-before-r$round.log" 2>&1
    taskset -c "${BENCHMARK_CPU:-0}" binaries/after --bench "git_defaults/$benchmark" --baseline "before-r$round" > "logs/$benchmark-after-r$round.log" 2>&1
  done
  python3 - "$round" <<'COPYRESULTS'
from pathlib import Path
import shutil, sys, os
round = sys.argv[1]
root = Path(os.environ['CARGO_TARGET_DIR']) / 'criterion/git_defaults'
for path in root.rglob('*.json'):
    if path.name in {'estimates.json', 'sample.json', 'benchmark.json'} and path.parent.name in {f'before-r{round}', 'new', 'change'}:
        dest = Path(f'results-round{round}') / path.relative_to(root)
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, dest)
COPYRESULTS
done
