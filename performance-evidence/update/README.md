# Workspace inheritance discovery

Production baseline: `8d3c2269f0eab776abe53ba11a7fa2f6ac35360a` (`main`).
The production patch is `inheritance.patch`; it excludes all benchmark files and dependency changes.

`inheritance.rs` is a supplemental Criterion harness modeled on #3146's temporary 10/100-package workspaces. `extract_inheritance.py` copies the inheritance-discovery statements verbatim from the production updater and its workspace-version read from `new_workspace_version` on the baseline. It wraps only those extracted statements in a callable function. The sole binding adaptation changes `for package in &workspace_packages` to `for package in workspace_packages`, because the wrapper borrows the fixture slice rather than owning its already-created vector; iteration and work remain identical without timing any metadata clones. Metadata generation, fixture creation, output checks, package cloning/relocation and version calculation are excluded. Manifest reading/parsing, version inheritance selection, and workspace-version lookup are timed, including result drops. Both revisions use the same fixture generator.

The independent cases are the optimized workload. Inherited cases preserve full scanning; member-manifest controls check that the public library's ability to select a package manifest does not cause inheritance discovery to be skipped. The harness asserts the discovered version and number of inheriting packages before measuring. Six cases use 40 samples, 1-second warmup, and 3-second target measurement.

`update.rs` in the temporary CLI benchmark overlay is unchanged from PR #3143 (`origin/benchmark-3143`). The original `workspace_10_noop` and `workspace_10_one_change` cases provide whole-command controls. All six original fixtures also run once in test mode and assert resulting versions/changelogs.

`cli_fixture.py` separately constructs the original no-op shape with 10 and 100 members. The actual optimized CLI binaries are traced with `strace -f -c` (all subprocess syscall summaries) and `strace -e trace=openat` (main-process manifest reads). These are diagnostic syscall counts, not elapsed-time benchmarks. The current repository remains clean after the update.

`run-comparison.sh` records the session commands and holds `/workspace/perf-build.lock` throughout all builds and timed before/after measurements. It first restores main's lockfile, adds Criterion with normal Cargo resolution, and asserts every original package name/version/source remains present. `lock-versions-check.txt` records that assertion. Builds and tests otherwise use `--locked`. An earlier exploratory build used an over-resolved lockfile; it was discarded without collecting measurements and is not the baseline.

All build paths refer to this execution workspace. Binaries and the metadata JSON are local debugging artifacts and need not be published. `inheritance-source-before.rs` and `inheritance-source-after.rs` show the exact extracted code measured; the original production patch is also preserved. Raw logs and Criterion estimates accompany the evidence.

## Observed results

Measured on Linux x86_64, Rust 1.99, optimized release builds with main's locked production dependency versions and only additive Criterion dependencies. One before/after run was enough for the focused cases: the 95% time intervals were 25.358–27.150 µs vs 2.1023–2.1618 µs for 10 independent packages, and 233.68–236.82 µs vs 2.0987–2.1285 µs for 100. Reductions below are computed from Criterion slope estimates; whole-command cases use mean estimates because Criterion chose flat sampling.

| Workload | Before | After | Observed reduction |
| --- | ---: | ---: | ---: |
| Independent workspace, 10 members (inheritance discovery only) | 26.143 µs | 2.1248 µs | 91.9% |
| Independent workspace, 100 members (inheritance discovery only) | 235.10 µs | 2.1125 µs | 99.1% |
| Original CLI workspace_10_noop | 539.56 ms | 540.59 ms | No statistically detected change (p=0.95) |
| Original CLI workspace_10_one_change | 562.02 ms | 574.86 ms | No statistically detected change (p=0.50) |

Inherited-workspace and member-manifest controls still scan all members and retain the same results. All six original CLI benchmark fixtures passed their assertions before and after. Real CLI `openat` traces show 20→10 original member-manifest opens for 10 members and 200→100 for 100 members, eliminating exactly one read per member (the remaining reads copy the source repository). Traced runs left the fixtures unchanged. This is a focused reduction in repeated manifest I/O/parsing; no overall `update` latency improvement was detected.

`results.json` contains exact point estimates and computed reductions; `criterion/` preserves estimates and raw sample JSON. `reproduce.sh /path/to/release-plz /path/to/new-comparison-directory` reconstructs the external benchmark overlay in a fresh clone, generates the exact before/after snippets from source, checks additive-only dependency resolution, and runs the comparison. It needs Python 3.11+, Cargo/Rust suitable for the repository, Git, and ordinary crate network access. The source repository is read-only. The code diff itself is commit `45c9bdee` and contains no benchmark files or dependencies.
