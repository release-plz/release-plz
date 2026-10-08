#!/usr/bin/env python3
"""Run three alternating comparisons of already-preserved optimized binaries."""

import os
from pathlib import Path
import subprocess


root = Path(__file__).resolve().parent
environment = os.environ.copy()
environment["CARGO_NET_OFFLINE"] = "true"
orders = [
    ["baseline", "indexed", "pathless"],
    ["pathless", "indexed", "baseline"],
    ["baseline", "pathless", "indexed"],
]
for run, order in enumerate(orders, start=1):
    for name in order:
        print(f"Run {run}: {name}", flush=True)
        with (root / f"{name}-{run}.log").open("w") as log:
            subprocess.run(
                [
                    str(root / f"core-tests-{name}"), "benchmark_dependency_propagation",
                    "--ignored", "--nocapture", "--test-threads=1",
                ],
                env=environment, stdout=log, stderr=subprocess.STDOUT, check=True,
            )
