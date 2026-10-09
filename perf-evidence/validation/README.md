# Combined validation

The seven independent production commits listed in `../changes.json` applied cleanly to main `267b5775ee1d52e9d22460fc5b65babe4f681f52`. `combined.patch` contains their combined production diff. Benchmark files and dependency changes were kept in a separate worktree.

Validation from the clean combined production tree:

```sh
cargo test --target-dir /workspace/release-plz/target --locked --all-features --workspace
cargo clippy --target-dir /workspace/release-plz/target --locked --all-targets --all-features --workspace -- -D warnings
cargo fmt --all --check
```

Result: 463 tests passed, 0 failed, 3 existing tests ignored. The local Gitea/Postgres services were healthy; all 158 CLI integration tests ran, including both added regressions. Clippy with warnings denied and formatting passed. Use your own target directory or omit `--target-dir` when reproducing.

The first attempt exported `CARGO_TARGET_DIR`, causing an existing package-extraction fixture to look for its archive in the wrong directory. The targeted recheck and complete rerun passed after using Cargo's `--target-dir` option so fixture subprocesses retained their normal paths. Both logs are retained.

For the separate original-benchmark smoke check, apply `benchmark-overlay.patch` to the combined tree, create `crates/release_plz/benches`, and copy `../reference/{update,manifest_updates}.rs` into it. Run:

```sh
cargo bench --locked --no-default-features --features all-static -p release-plz --bench manifest_updates --bench update -- --test
```

This executes the original #3146 benchmark assertions without collecting new timing estimates. See `original-benchmarks-smoke.log`. The earlier statistical before/after measurements are in the individual evidence directories.
