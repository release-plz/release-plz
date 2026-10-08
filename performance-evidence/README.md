# Performance evidence for release-plz follow-up optimizations

Production baseline: `6e42d3c20d11a5aee4b046c0a7e4f35f7e15d506` (`main`). Original benchmark source: PR #3146 at `517963a958d3cf693ac83f270e3e648a4b4949d2`, including the benchmark infrastructure from PR #3143.

This branch holds benchmark sources, locked harness dependencies, reproduction scripts and raw results. None of these files are in the production pull requests. Each proposed change is independently based on main and was measured three times before and after, using saved optimized binaries and identical inputs. Builds and other benchmarks were paused during measurements. See environment.json and each directory's README/protocol for details and limitations. Percentages are operation-specific unless the benchmark runs the full update command.

- `version`: original next-version workloads, early exit after a decisive breaking commit.
- `conventional`: classification using the same borrowed-message parser as git-cliff.
- `changelog`: PR-number regex caching and avoiding unused header captures.
- `files`: real public package comparison with bounded file reads.
- `manifest`: inherited dependency lookup and independent-package dependency propagation.
- `templates`: direct formatting of default Git tags and release names.
- `history-investigation`: two subprocess-reduction candidates left unpublished after inconsistent end-to-end timings; includes profiles, source patches and all three before/after runs.

Some harnesses include exact production modules to reach private functions. Their READMEs describe source refresh and any stubs for unrelated types. Full-command and public-method harnesses invoke the actual library or CLI. No SaaS or external registry is used by the timed fixtures.

## Validation

The complete set of candidate changes passed `cargo test --locked --no-default-features --features all-static --workspace`: 328 passed, 83 ignored (Docker-dependent tests disabled). It also passed `cargo clippy --locked --all-targets --all-features --workspace -- -D warnings` and `cargo fmt --all -- --check`. These broad checks ran on a combined temporary checkout; individual benchmark assertions and focused tests are recorded in the respective directories. `validation/combined.patch` reproduces that checkout from the baseline. The two history changes overlap at one method: the combined version preserves the cached-file path and the optional commit-message return.

Cargo used `--config 'build.target-dir="/workspace/release-plz/target"'` for the shared cache. A preliminary test invocation instead exported CARGO_TARGET_DIR, which caused a nested `cargo package` test to place its archive outside the fixture's expected directory. The corrected complete rerun passed; both logs are retained. Shared-target builds explicitly refreshed changed-source mtimes and checked rebuilt artifacts/binary hashes to avoid stale Cargo reuse across worktrees.
