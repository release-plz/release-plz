#!/usr/bin/env bash
set -euo pipefail
export CRITERION_HOME=/workspace/perf-results/version/criterion
bench=/workspace/scratch/perf-version/target/release/deps/next_version-ef788a84e15d488a
cd /workspace/scratch/perf-version
for run in 1 2 3; do
  for revision in before after; do
    "$bench" --bench "next_version/$revision/" --save-baseline "run-$run-$revision" --noplot > "/workspace/perf-results/version/run-$run-$revision.log" 2>&1
    echo "Version run $run $revision complete"
  done
done
