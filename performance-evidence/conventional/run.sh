#!/usr/bin/env bash
set -euo pipefail
for run in 1 2 3; do
  for revision in before after; do
    python3 /workspace/perf-results/conventional/run-one.py "$revision" "/workspace/perf-results/conventional/run-$run-$revision"
    echo "Conventional run $run $revision complete"
  done
done
