# Performance follow-up evidence

All seven production branches start at release-plz main commit `267b5775ee1d52e9d22460fc5b65babe4f681f52`. The changes and branch names are listed in `changes.json`. None includes benchmark code, benchmark dependencies, or the optimization from PR #3184.

This branch stores the separate measurement harnesses, raw observations, reproduction instructions, binary hashes, and validation logs. Benchmarks ran in optimized builds on the same machine, with three before/after repetitions for every proposed change. Timing was serialized: no other agent compiled or benchmarked concurrently. CPU and compiler information is in `environment.json`.

- `changelog/`: exact production source snapshots, portable Cargo harness, all paired samples, and existing-test results for lazy error formatting and default configuration construction.
- `update/`: temporary in-module instrumentation for version groups, changelog inclusion snapshots, and Git-client construction. Includes the separate authenticated CLI comparison, which did not establish a consistent whole-command improvement.
- `manifests/`: temporary in-module harnesses extending PR #3146's workspace fixtures for dependency propagation and manifest updates, including real reads and writes.
- `reference/`: the original benchmark files at PR #3146 commit `517963a958d3cf693ac83f270e3e648a4b4949d2`, plus the prior offline CLI fixture adaptation used for smoke checks and process profiling.
- `validation/`: the combined production patch, workspace tests, lint results, and smoke checks of the original benchmark suites. The benchmark overlay lives only in a separate validation worktree.

Component timings should not be read as whole-command speedups. PR bodies explicitly identify their measured scope and report controls, including the extra cost for dense, empty changelog includes and the small whole-render effect of default configuration construction. Fixture setup and correctness checks are outside timed loops unless the individual methodology states otherwise.

No benchmark binaries are stored in Git. Rebuild them using the instructions and scripts in each evidence directory. The production PRs can be merged independently of this evidence branch and benchmark PRs #3143/#3146.
