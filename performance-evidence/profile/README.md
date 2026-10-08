# Repository-copy and release-order benchmarks

This directory contains standalone measurement harnesses for two small performance patches against release-plz main commit `72465264`. They deliberately live outside the production PRs.

`harness/src/copy_before.rs` and `order_before.rs` are exact source snapshots from that main commit. `copy_after.rs` and `order_after.rs` are exact snapshots from the proposed patches. Compiling both variants together means compiler options and dependency versions are identical. The harness invokes those production functions directly. `Cargo.lock` records dependencies used for measurements.

Run with a current stable Rust toolchain, Python 3 and Git:

```sh
./run-benches.sh
```

On a shared machine serialize the whole command with other builds and benchmarks (for example, `flock /tmp/release-plz-bench.lock ./run-benches.sh`). `CARGO_TARGET_DIR` can point to a shared build directory. The script performs three runs per variant, reverses their order for the second run, and writes raw results under `results/`.

The repository-copy workload creates clean Git repositories with 100 and 1,000 tracked Rust source files spread across nested crate/source/module directories, packs their Git objects, and excludes generated `target/` content. Each reported run is the mean of 20 repository copies after two warmup copies. Destination creation, destruction, and validation are excluded from the measured interval. Every copy verifies its Git status, every tracked file's contents, and exclusion of ignored output. Output columns are variant, iterations, microseconds per copy.

The release-order workload creates dependency chains of 1, 10, 100 and 500 workspace packages, each with 20 external dependencies. Packages are supplied in reverse dependency order to exercise recursive traversal. Fixture construction and validation are excluded. Each run measures repeated order calculation for two seconds; output columns are package count, variant, iterations, nanoseconds per operation. Returned order is checked against the complete expected chain before timing.

These are focused component measurements. They do not imply the entire `update` command improves by the same percentages. The copy helper is used when creating the temporary repository for updates and release PRs; release ordering is used when constructing the project. `strace` summaries, when available, count metadata syscalls for the copy workload; tracing results are separate from timing results.
