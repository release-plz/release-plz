#!/usr/bin/env python3
"""Append temporary benchmarks; run only in disposable worktrees.

Usage: python3 inject-harness.py WORKTREE release-order|owned-diffs before|after
"""
from pathlib import Path
import sys

worktree = Path(sys.argv[1]).resolve()
candidate, side = sys.argv[2:]
evidence = Path(__file__).resolve().parent
if candidate == "release-order":
    path = worktree / "crates/release_plz_core/src/release_order.rs"
    harness = (evidence / "release_order_harness.rs").read_text()
else:
    assert candidate == "owned-diffs"
    path = worktree / "crates/release_plz_core/src/command/update/updater.rs"
    harness = (evidence / "fill_commits_harness.rs").read_text().replace(
        "BENCH_INPUT", "input" if side == "after" else "&input"
    )
assert side in {"before", "after"}
assert "mod perf_bench_" not in path.read_text(), "harness already installed"
path.write_text(path.read_text() + harness)
