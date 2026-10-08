# Local performance evidence

This branch contains temporary benchmark code and raw measurements. It is deliberately separate from the production pull requests and is not intended to be merged. Production comparisons use `9b9d95ab` (main at the start of the investigation) plus exactly the optimization named in each PR.

The starting benchmark infrastructure and CLI fixtures come from PR #3143 (`30a18146ecfb756a7bd5aac1514d14aa4255d696`); manifest fixtures come from PR #3146 (`517963a958d3cf693ac83f270e3e648a4b4949d2`). Supplemental fixtures isolate specific operations.

Environment: Linux x86_64, Intel Xeon Platinum 8573C, four CPU quota, Rust 1.99.0. Optimized Cargo bench/release profiles, default features, fixed Cargo.lock. Builds and measurements were serialized. Local workspace sources were forced to recompile when switching checkouts sharing a target directory; standalone before/after executables were preserved. Fixture setup and correctness validation are excluded from timings. Each accepted change has three before and three after runs.

For a reproduction, check out this branch in a disposable checkout, run the selected benchmark, cherry-pick only the production commit from its PR, and run the identical benchmark again. Repeat three times. Keep the same target directory within that checkout, and do not run other builds during measurements. Raw Criterion logs retain the reported confidence intervals and central estimates. Summary reductions in PRs use `1 - median(after) / median(before)`.

## Changelog builder

```sh
cargo bench --locked -p release-plz --bench builder
```

Measures `ChangelogBuilder::build()` for a configured changelog containing 1, 10, or 1,000 conventional commits. Each iteration receives a fresh builder clone outside timing, avoiding reuse of warmed regex caches. Generation and output assertions run outside timing. This is a focused construction benchmark, not an end-to-end CLI result. Raw runs are in `changelog-builder/`.

## Workspace HEAD restoration

```sh
cargo bench --locked -p release-plz --bench update -- 'update/workspace_10_(noop|one_change)'
```

Uses the original PR #3143 CLI fixtures. Versions, changelogs and clean no-op output are validated after every timed command. Three comparisons alternate before/after order, with the second pair reversed. The supplemental environment variable `RELEASE_PLZ_BENCH_BINARY` allows selecting an immutable CLI executable; absent it, the benchmark uses Cargo's current release-plz executable. Raw untraced timing runs and separately collected strace command counts are in `checkout/`. The standalone `profile_update.py` reproduces the CLI fixtures for profiling.

## Cached history file lists

```sh
cargo bench --locked -p release-plz --bench update -- history_100
```

The original PR #3143 fixture runs the real CLI against 100 fix commits, checks the new version and every changelog entry, and resets the disposable repository outside timing. Three before/after pairs use immutable executables, with the second pair reversed. `history/` retains raw means and separate untimed tracing results: successful Cargo.lock checkout commands fall from 201 to 101.

## Private configuration and workspace-version methods

Two ignored tests are present only on this evidence branch to call the actual private methods without exposing new production APIs:

```sh
cargo test --locked --release -p release_plz_core --lib performance_experiment::bench_config_lookup -- --ignored --nocapture --test-threads=1
cargo test --locked --release -p release_plz_core --lib performance_experiment::bench_workspace_version -- --ignored --nocapture --test-threads=1
```

Configuration lookup uses workspace defaults with four configured strings, an explicit package override, and the default fallback. Each reported sample averages 8,000,000 calls after warm-up. Workspace-version selection uses 100/1000 packages with all, one quarter, or none inheriting the shared version. It measures the actual method, including its manifest read, and validates the selected version. Fixture creation is excluded; inputs/results are black-boxed. Three interleaved before/after pairs reverse order in pair 2. Raw runs and exact methodology are in `private-methods/`. Zero-inheritor controls are retained, including the observed slower control; these focused results do not establish whole-command speedups.

## Dependent manifest filtering

```sh
cargo bench --locked -p release-plz --bench manifest_updates
```

Extends the manifest benchmark from PR #3146 with 10- and 100-package dependency chains, retaining the independent 100-package control. Each call updates package-0 with alternating versions; dependent and unrelated requirements are checked outside timing. Metadata and fixture creation are excluded. Three pairs reverse order in pair 2. Raw runs and complete slope-estimate tables are in `manifest/`. The chain cases improve in all three pairs; the independent control has a 13.61% slower median and mixed per-pair direction. These are manifest-editing measurements, not whole-command speedups.

Two further candidates were rejected: broader changelog configuration ownership regressed measured workloads, and coalescing file metadata queries did not improve consistently across three controlled runs. No production PR was opened for either.
