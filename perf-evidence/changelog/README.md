# Changelog performance evidence

Base: release-plz `267b5775`.

Independent production commits: `11cff443` (lazy remote error formatting) and `a005b433` (construct defaults once). Both have the same main parent. The full exact source is retained as before.rs, after-remote.rs and after-defaults.rs. No benchmark code is present in either production commit.

bench.rs includes those actual production modules and adds private-component harness entry points via helpers.rs. Each of three new processes validates identical output before measuring. Every case warms both variants for 100 ms, then alternates before/after order over 21 pairs of at least 100 ms blocks. Each block measures batches of ten operations, including destruction; reported values are medians of the 21 block times. CPU-intensive builds/benchmarks were serialized across agents. run-{1,2,3}.log retain all block-level values. summary.json records process medians, reductions, the measured binary hash and timing scope.

- `remote_context_N` repeatedly inserts remote context with N contributors into a prepared renderer. Each contributor has a username and two PR numbers. Previous JSON values are dropped during replacement.
- `remote_generate_N` measures the public builder `build()` plus `generate()` with the default changelog template, one fix commit, and N remote contributors.
- `default_config` measures constructing config through the actual private `changelog_config` method, without a user-supplied config or PR link.
- `default_generate` measures builder `build()` plus `generate()` with one fix commit and default config.
- `default_prepend_pr` measures builder `build()` plus `prepend()` with one fix commit, a PR link and a prior release.

Build used Rust release dependencies built by the parent with `cargo build --release --locked -p release-plz --no-default-features --features all-static` at the base revision. build.log records the complete optimized rustc command and every exact dependency artifact. build.py can rebuild in this workspace, while Cargo.toml and Cargo.lock provide an independent portable Cargo alternative: `cargo run --release --locked --manifest-path <evidence>/changelog/Cargo.toml`. The standalone lockfile was generated offline from the cached dependency index; all shared package versions match the baseline repository lockfile.

Reproduce here:

```sh
source /workspace/.devtools/activate.sh
python3 /workspace/perf-evidence/changelog/build.py > /workspace/perf-evidence/changelog/rebuild.log
for run in 1 2 3; do
  /workspace/perf-evidence/changelog/benchmark > /workspace/perf-evidence/changelog/run-${run}.log
done
python3 /workspace/perf-evidence/changelog/summarize.py
```

The benchmark snapshots also retain all existing production changelog unit tests. `build.py --test` compiles them together using the repository's expect-test dev dependency; `unit-tests` runs all three module copies and the unchanged parser tests. The exact-source test executable ran all 32 existing-test executions successfully across the three changelog variants and the unchanged parser; see test-build.log and test.log.
