# Performance evidence, 2026-10-10

This branch contains local benchmark harnesses and measurements for independent
performance changes based on main commit
`74b20485f891fec3a9591b17d0c38dd5d99e4d07`. It is an evidence branch, not a
dependency of the production PRs. None of these harnesses or dependency changes
are included in those PRs.

The original Criterion harness comes from
[PR #3146](https://github.com/release-plz/release-plz/pull/3146), including its
base [PR #3143](https://github.com/release-plz/release-plz/pull/3143).
Supplemental cases isolate the work changed by each optimization. They assert
the resulting versions, selected dependencies, file equality, or rendered
changelog content. Full CLI cases use disposable repositories and validate
versions and changelogs after each timed update.

Environment: Linux x86_64, five available AMD EPYC 9V45 vCPUs,
`rustc 1.99.0 (b940084d7 2026-09-28)` and Cargo 1.99.0. Release/bench builds
use the standard optimized profile. Builds, tests, and timed workloads were
serialized with a shared lock. Sources were rebuilt when switching variants;
compiled baseline and candidate executables were retained for paired repeats.
Third-party dependency builds and fixture creation are outside measured regions.

Each candidate is measured independently against the same main commit. Reported
improvements are reductions in elapsed time, `(before - after) / before`. They
apply to the named workload, not automatically to an entire release workflow.
Improvements from different PRs must not be added together.

Use separate disposable checkouts of the baseline and the desired PR head to
reproduce. Keep this evidence checkout outside both, apply the relevant harness
patch in each, and use identical toolchain/build flags. The patches are local
instrumentation, so do not commit them into the production branches. Run only one
benchmark process at a time. If sharing a Cargo target directory between
checkouts, force the affected workspace crate to rebuild before each variant.

Per-area reproduction instructions, exact timing logs, and results are stored
in the corresponding directories. Criterion JSON estimates/sample files are
included where applicable; custom timing logs include every batch sample.

Validation of the combined production changes used the local Docker-backed
Gitea/PostgreSQL test services. The initial full workspace run passed all 159 CLI
integration tests and 39 CLI unit tests. A core archive-location fixture needed
the inherited `CARGO_TARGET_DIR` removed; subsequent tests use Cargo's
`--target-dir` option instead so subprocess fixtures retain their own target
directories. Final validation results are recorded in `validation.md`.
