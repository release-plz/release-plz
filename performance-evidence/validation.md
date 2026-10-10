# Validation

The seven independent production changes were also applied together to main
`74b20485f891fec3a9591b17d0c38dd5d99e4d07` and checked without any benchmark
instrumentation. `combined-production.patch` preserves that tested source diff.
The investigated HEAD-checkout optimization was removed before this final run.

All checks passed:

```sh
cargo test --locked --all-features --workspace
cargo clippy --locked --all-targets --all-features --workspace -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Tests: **466 passed, 0 failed, 3 existing tests ignored**, including 159 CLI
integration tests, 39 CLI unit tests, 184 core unit tests, and the version tests
and doc tests. Docker-backed integration tests used the repository's local
Gitea 28.1.0 and PostgreSQL 18 services.

Cargo received the shared build directory through `--target-dir`, with
`CARGO_TARGET_DIR` unset, to avoid changing target paths in subprocess fixtures.
The earlier environment-related archive-location failure passed after this
correction. The final full suite above completed successfully in one run.

Individual candidate checkouts also ran focused tests, described in each PR.
The production commits contain no benchmark dependencies, harnesses, lockfile
changes, temporary visibility changes, or allocation instrumentation.

During the investigation main advanced to
`184b30140260459811e60b604b54911dcfea77fe` (`chore: release (#3135)`). That commit
changes package versions, corresponding lockfile entries, and changelogs only;
the benchmarked production source is unchanged. PRs target `main`, while the
measurements pin their before/after comparison to `74b20485` for reproducibility.
