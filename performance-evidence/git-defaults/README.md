# Lazy git changelog defaults

This standalone harness compares the exact production `changelog.rs` at main
`8d3c2269` (`snapshots/before.rs`) with the lazy-defaults candidate
(`snapshots/after.rs`). The production PR does not include this harness or its
dependencies. `src/changelog_parser.rs` is copied unchanged from that main commit.

Run from this directory with Rust and Cargo on PATH:

```sh
cp snapshots/before.rs src/changelog.rs
cargo bench --locked --bench git_defaults -- --save-baseline git-defaults-before
cargo run --locked --release --example allocations
cargo run --locked --release --example semantics > before-semantics.txt
cp snapshots/after.rs src/changelog.rs
cargo bench --locked --bench git_defaults -- --baseline git-defaults-before
cargo run --locked --release --example allocations
cargo run --locked --release --example semantics > after-semantics.txt
cmp before-semantics.txt after-semantics.txt
cargo test --locked --release
```

The configured phase benchmark consumes a populated `GitConfig`, with setup
cloning excluded from timing. The empty-config phase exercises the fallback
path as a control. Full benchmarks run `ChangelogBuilder::build().generate()`
with 1 or 20 commits, a release date, release link, previous version and remote
metadata containing five contributors. The default full-render case uses no
explicit config. These measure local changelog work, not network-dependent
`release-plz update` elapsed time. Each case uses 100 samples, 1-second warm-up,
and 3-second measurement.

The allocation example warms the lazy regex statics, then counts allocations
and requested bytes only during the default-application phase. The semantics
example compares all 32 combinations of empty/custom parser, preprocessor, sort
and link-parser fields with and without a PR URL. The test command includes the
production changelog and parser module tests.

Timings are Criterion `mean.point_estimate` values, in nanoseconds, from the
retained `results/*/{git-defaults-before,new}/estimates.json` files. Inputs and
sample data are retained alongside them. Benchmarks ran serially under a shared
build lock on an AMD EPYC 9V45 VM with Rust 1.99.0. Dependency versions were
seeded from main's lockfile, adding Criterion 0.8.2 with default features disabled.

## Observed results

The configured default-application phase was 95.96–96.44% faster across three
runs. It eliminated 36 allocations and 15,781 requested bytes per call. The empty
fallback retained the same 36 allocations and requested bytes.

Round 1 showed large drift in unchanged controls. Rounds 2 and 3 therefore ran
each case in a fresh process pinned to CPU 0 (`repeat-isolated.sh`). The full
builder/render measurements do not establish a consistent overall speedup or
regression; the performance claim is limited to default application and its
allocations. The third round detected no significant timing change in either
custom full-render case.

All values below are before → after means in microseconds. Exact nanosecond
values and percentage changes are in `summary.json`.

| Case | Round 1 | Round 2 | Round 3 |
| --- | ---: | ---: | ---: |
| build_generate_custom_1 | 184.935 → 185.918 | 192.257 → 199.560 | 190.188 → 189.273 |
| build_generate_custom_20 | 594.653 → 654.012 | 604.868 → 570.292 | 590.856 → 589.993 |
| build_generate_default_control_1 | 267.243 → 371.944 | 183.762 → 183.973 | 186.623 → 180.105 |
| configured_phase | 1.297 → 0.046 | 1.186 → 0.044 | 1.267 → 0.051 |
| empty_phase_control | 1.109 → 1.732 | 1.426 → 1.492 | 1.454 → 1.458 |

Validation: all 16 production module tests pass; all 32 configuration-output comparisons match exactly.
