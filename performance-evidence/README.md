# Performance measurements

This branch contains measurement tools and results only. The production pull
requests target `main` independently and do not include these benchmarks.

Baseline: `72465264edf1d99b35e5b02bb01dd71b525b88f8`.
The common Criterion benchmarks come from PR #3146 at
`517963a958d3cf693ac83f270e3e648a4b4949d2`.

Measurements were run on Linux x86-64 with Rust/Cargo 1.99.0, an Intel Xeon
Platinum 8573C, and five available CPUs. All builds, tests and benchmark runs
shared an exclusive lock. Each reported comparison has three runs per variant.
Headline reductions use the median of the three per-run estimates:
`100 * (1 - after / before)`. These are observed workload timings, not a claim
that every invocation of release-plz improves by the same amount.

## Criterion workloads

This branch has the benchmark targets installed on the baseline production code.
To reproduce, create a disposable worktree at this branch and run the relevant
benchmark command three times. Then apply the production commit under review
and repeat three times with the same toolchain, flags and machine. Avoid other
CPU-intensive work during the comparison.

Use one worktree for the sequential before/after builds, or a separate Cargo
target directory for each worktree. Cargo can reuse stale local-crate artifacts
when several worktrees share a target directory. For our shared-target runs,
local crates were explicitly invalidated when switching worktrees and build logs
were checked for recompilation. The first saved baseline binaries were built
before any production change. An invalid manifest experiment caught during this
check was discarded and is not included in the results.

```sh
cargo bench --locked -p next_version --bench next_version -- \
  --warm-up-time 1 --measurement-time 2 --sample-size 30 --noplot
cargo bench --locked -p release-plz --bench manifest_updates -- --noplot
cargo bench --locked -p release-plz --bench update -- \
  'history_100|workspace_10_one_change' --noplot
cargo bench --locked -p release-plz --bench changelog_render -- \
  --warm-up-time 1 --measurement-time 2 --sample-size 30 --noplot
```

`next_version` measures version calculation for borrowed commit messages.
`manifest_updates` measures manifest processing with fixture creation and Cargo
metadata outside the clock. `update` launches the actual CLI against disposable
offline repositories and verifies resulting versions and changelogs.
`changelog_render` measures default changelog configuration and complete rendering
in one process; lazy caches are warm after Criterion warmup.

The history experiment saves each CLI binary and alternates them between runs.
The update benchmark embeds the CLI path at compile time, so when reproducing with
saved binaries, replace the executable at that path for each variant. Rebuilding
each variant normally with Cargo also selects the correct executable.

## Repository copy and release ordering

The standalone harness under `profile/` contains exact before/after copies of the
two measured production modules. It can compile both variants into the same
optimized executable and alternate them without repeated dependency builds.

```sh
bash performance-evidence/profile/run-benches.sh
```

Repository-copy measurements exclude fixture creation, destination cleanup and
correctness checks. Every copied tracked file and Git status are checked after
each iteration. Release-order measurements validate the expected package order
before timing and keep fixture construction outside the clock.

Raw results and per-change summaries are stored alongside the harnesses. The PR
descriptions record the relevant before/after triples, limitations and tests.
