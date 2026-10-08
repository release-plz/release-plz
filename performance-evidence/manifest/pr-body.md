Updating one package currently reads and parses every workspace member's manifest to look for dependency requirements to rewrite. Use the existing Cargo metadata to skip member manifests without path dependencies. The workspace manifest is always inspected because its dependency templates may be unused by any package.

Cargo metadata includes renamed, target-specific, build, dev and inherited path dependencies. The manifest inspection and version-requirement rewrite remain unchanged for those packages; the filter only checks whether a path exists, so metadata paths from a relocated checkout do not affect the decision.

Performance measured against `main` at `72465264` using the unmodified `manifest_updates` Criterion benchmark from #3146. Both release-profile binaries were built with `cargo bench --locked -p release-plz --bench manifest_updates --no-run`. All workspace crates were rebuilt for the changed binary to avoid sharing stale worktree artifacts. Three before/after pairs ran alternately, with one exclusive build/benchmark lock and no concurrent build or benchmark jobs. Fixture creation and Cargo metadata are outside the timed section. Each iteration changes one package version, persists its manifest and updates dependent requirements; the fixture verifies the resulting version. These numbers measure manifest processing, not the entire `release-plz update` command.

| Workload | Before, runs 1 / 2 / 3 (µs) | After, runs 1 / 2 / 3 (µs) | Median time decrease |
| --- | --- | --- | --- |
| 10 independent members | 55.74, 56.07, 74.80 | 13.74, 13.89, 16.26 | 75.2% (56.07 → 13.89 µs; 4.04× faster) |
| 100 independent members | 439.26, 441.84, 464.60 | 14.66, 14.00, 16.04 | 96.7% (441.84 → 14.66 µs; 30.14× faster) |

Each value is Criterion’s central time estimate for a complete benchmark run; the decrease compares the median of three runs. Linux x86-64, Intel Xeon Platinum 8573C, rustc 1.99.0, release/bench profile.

Validation: `cargo fmt --all -- --check`; `cargo test --release --locked -p release-plz --test all set_version -- --nocapture`. A regression test covers a renamed target-specific build dependency and an unused workspace dependency template.

No benchmark code or benchmark dependencies are included in this PR; it can merge independently of #3146.

[Reproduction instructions and raw results](https://github.com/release-plz/release-plz/tree/codex/performance-evidence-20261008/performance-evidence/manifest) are on a separate evidence branch.
