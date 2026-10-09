# Release-plz performance evidence

This branch contains benchmark source and measurements only. It is not intended to be merged. The eight production branches listed below each start from `main` at `03a7393dc40f48bbaa6ceb776a4e95dfc04db627` and contain no benchmark code or dependency changes.

The starting suite is [PR #3146](https://github.com/release-plz/release-plz/pull/3146). The CLI and root-manifest cases adapt that suite; focused real-code harnesses cover other operations. None of these changes implements the version-increment optimization in [PR #3181](https://github.com/release-plz/release-plz/pull/3181).

Each reported case has three before/after runs. Builds, tests, profiles, and measurements ran serially under one machine lock. Binaries were preserved independently. Shared-target Cargo artifacts initially proved stale across worktrees; those invalid attempts were rejected, and final cross-worktree measurements clean local package artifacts before compiling and validate source paths, hashes, and workload outputs. The release-body pair uses uniquely named examples containing exact source snapshots in one compilation.

| Change | Production commit | Evidence |
| --- | --- | --- |
| skip-empty-history | [`ad2dd0f4`](https://github.com/release-plz/release-plz/commit/ad2dd0f4e1289067f32a597b454affc9332922dd) | [git](git/) |
| share-history-head | [`cfeedee8`](https://github.com/release-plz/release-plz/commit/cfeedee8abdafb4a3d164d160896480fd8ca1cb3) | [git](git/) |
| default-release-body | [`8bdfdda7`](https://github.com/release-plz/release-plz/commit/8bdfdda7120b23e14f6466b3ee9833a047835ed6) | [release-body](release-body/) |
| manifest-iterator | [`9b5f7825`](https://github.com/release-plz/release-plz/commit/9b5f782504382686ca94282dc8fc6e9d0ed97417) | [manifest-iterator](manifest-iterator/) |
| root-manifest | [`bf225fb1`](https://github.com/release-plz/release-plz/commit/bf225fb1563e3a6c89358be90291f8488522e9a9) | [root-manifest](root-manifest/) |
| commit-metadata | [`7532f156`](https://github.com/release-plz/release-plz/commit/7532f156cc35a252a39875a476c56b7f3c49b9e6) | [commit-metadata](commit-metadata/) |
| release-order | [`5625847f`](https://github.com/release-plz/release-plz/commit/5625847f434d7f431ce4cd59573a6531268b4364) | [core-update](core-update/) |
| owned-diffs | [`d5b405f5`](https://github.com/release-plz/release-plz/commit/d5b405f52fc2dbee6f576d5c5bd082a3fcb3e8fb) | [core-update](core-update/) |

Run the `reproduce.sh` script in each evidence directory with separate baseline and candidate checkouts. The Git cases use `git/reproduce_git.sh`; the exact #3146 benchmark dependency overlay is retained in `git/overlay/`. Scripts document their arguments. Raw result files include iteration counts and all unrounded estimates. `environment.json` and `rustc.txt` describe the machine and compiler.

CLI results measure complete offline `release-plz update` invocations. Other results measure only the named operation and are not whole-command speedups. The release-order optimization deliberately exposes its small-workspace overhead as well as its large-workspace gains. Effects from independent changes should not be added together.

Baseline smoke tests cover all six original update fixtures and both original manifest fixtures. `profiles/` records subprocess profiling of unchanged 10-package and 100-commit workloads. The dominant remaining costs are Cargo package listing and Git subprocesses. These traced durations are diagnostic, not benchmark speedup claims.

The renderer ownership candidate was rejected after inconsistent repeated measurements; see [the decision](rejected-renderer/decision.txt). No production branch or PR contains that candidate.

Combined validation against the newer main `d1e29d97` (which merged #3181 during this task) passed 340 tests with 83 existing ignored tests, plus workspace formatting and Clippy with all targets/features and warnings denied. Logs, commands and the combined patch are under [validation/](validation/). Two Git changes touch adjacent lines; that small textual overlap is resolved in the validation patch.
