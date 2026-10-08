# Manifest dependency filtering benchmark evidence

Production commit: `7109632c42dc5db875fb0e1a48baa28b5bcb9ee0` (`perf/manifest-processing`), based directly on main `9b9d95ab968305524ea2ff35dd3f86c9dbf09610`.

The patch uses resolved dependency package names from Cargo metadata to avoid reading member manifests that cannot depend on the package being updated. It always scans the workspace root (including unused dependency templates), handles renamed and target-specific dependencies, and conservatively falls back to the previous filter if the updated package is absent from the supplied package list. Dependency paths are deliberately not used for this prefilter because release-pr relocates manifests while dependency metadata still refers to the original checkout.

The temporary `manifest_updates.rs` benchmark is derived from PR #3146. It retains the independent/100 control and adds 10- and 100-package chains. Package i depends on package i-1, and each timed call updates package-0 through the real `release_plz_core::set_version` API. Version alternation forces manifest edits; post-measurement assertions check the target version, its direct dependent's requirement, and an unrelated dependency. Cargo metadata and fixture construction are outside the timer. This measures manifest editing, not the entire update command.

The same benchmark source and dependency lockfile were used for the main and changed builds. Both binaries were preserved separately. Shared-target builds touch all workspace Rust sources inside the CPU lock before compiling, avoiding cross-checkout Cargo timestamp reuse.

Each binary ran in three separate Criterion processes. Order was before/after, after/before, before/after. Every pair acquired `/workspace/perf-cpu.lock`. Criterion uses 100 samples, a 1-second warmup, and a configured 3-second measurement (extended automatically when needed). Reported values are Criterion's slope point estimates, matching the central times in the raw logs. Percentage changes compare the median of each variant's three point estimates.

Environment: Linux x86_64, Intel Xeon Platinum 8573C, 5 visible CPUs, rustc 1.99.0 (b940084d7 2026-09-28), release optimization.

See `summary.md` for all per-run numbers. The chain cases improve in every pair. The independent control has mixed per-pair direction and a slower after median, so there is no demonstrated improvement for workspaces without path dependencies; disclose its measured values alongside the chain wins.

Validation: 9 set_version integration tests passed. The added regression covers renamed dev and target-specific build dependencies using metadata from an original checkout while editing a copy, including the public API case where the supplied package list omits the updated package. Formatting and diff whitespace checks pass.

Local reproduction in the prepared clones:

```sh
/workspace/perf-results/manifest/build.sh before
/workspace/perf-results/manifest/build.sh after
/workspace/perf-results/manifest/test.sh
/workspace/perf-results/manifest/measure.sh 1
/workspace/perf-results/manifest/measure.sh 2
/workspace/perf-results/manifest/measure.sh 3
python3 /workspace/perf-results/manifest/summarize.py
```

The build command is `cargo bench -p release-plz --bench manifest_updates --locked --no-run`. Each copied executable is invoked with `--bench --noplot --save-baseline <before-or-after>-<run>`. The focused test command is `cargo test -p release-plz --release --no-default-features --features all-static --locked --test all set_version`.

Raw logs are `before-{1,2,3}.log` and `after-{1,2,3}.log`; Criterion sample data and estimates are under `criterion/`. Production changes are also saved as `production.patch`. Benchmark source SHA256: `105228bff0a8ba46b3b1057f1ddf5d742c1fff6823792a101a1829526432509e`. Benchmark lockfile SHA256: `deb4e4b6e5a72c04c3b25bffc46eca031abbfc0c3109740c2b9ef039a8be8856`.

No benchmark code, Criterion dependency, or lockfile changes are included in the production commit.
