# Stop parsing commits after a breaking bump is determined

Baseline: release-plz `6e42d3c2` (main); candidate: `96d3301a` (`codex/perf-stop-parsing-breaking-commits`).

`next_version.rs` is the nine-case Criterion benchmark from PR #3146, with the same versions, messages, commit counts, and correctness assertions. The only harness additions are revision aliases and explicit Criterion settings (1 s warmup, 2 s measurement, 50 samples). Baseline source is copied verbatim under `baseline/src`; candidate was built from its clean production worktree. The equivalent exact candidate source snapshot is packaged under `candidate/src` for portable reproduction with the included Cargo.toml/Cargo.lock and `benches/next_version.rs`. Run `cargo bench --manifest-path <this-directory>/Cargo.toml --bench next_version -- <filter> --save-baseline <unique-name> --noplot` to reproduce.

Build performed using:

```sh
source /workspace/.devtools/activate.sh
cargo bench --manifest-path /workspace/scratch/perf-version/Cargo.toml --bench next_version --no-run
```

Run command: `bash /workspace/perf-results/version/run.sh`. This launches separate before/after invocations in three alternating pairs, each with a unique Criterion baseline name. Criterion JSON data is in `criterion/`; all six complete stdout/stderr logs are preserved beside this file. No build ran during measurements. Reported times use Criterion mean point estimates, then the arithmetic mean of the three independent runs. All fixture creation happens outside the measured closure. This is a version-calculation microbenchmark, not an end-to-end update-command speedup.

Validation: `cargo test -p next_version --target-dir /workspace/scratch/perf-version-test-target` (4 unit +29 integration +17 doctests passed); `cargo fmt --all -- --check`; `git diff --check`.
