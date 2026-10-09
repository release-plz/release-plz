These measurements compare independent changes against main at `267b5775ee1d52e9d22460fc5b65babe4f681f52`. No candidate contains benchmark code. The commits are:

- `c9e418f0`: compile custom version-group rules only for grouped packages.
- `beb5632c`: snapshot only histories requested by changelog includes.
- `c2e2782e`: construct a Git client only when the changelog requires remote data.

`install_bench.py CHECKOUT` temporarily injects tests into the private updater module. It calls the actual `get_version_groups` and `fill_commits` methods. For snapshot preparation, it extracts that production block verbatim into a temporary helper and updates its original call site; no algorithm is duplicated. `client_bench.rs` and `group_control.rs` are companion injected test code. These files and all benchmark-only patches stay outside the performance PRs.

Build the instrumented core tests with:

```sh
cargo test --release --locked -p release_plz_core --lib --no-run --message-format=json
```

Copy the test executable reported by Cargo before rebuilding another checkout. The measured environment uses Rust 1.99.0 on Linux x86_64 / AMD EPYC 9V45, with the core crate's default `all-static` feature. Shared build targets were cleaned of all workspace crates before every candidate build to prevent cross-worktree stale artifacts. Binaries' SHA-256 hashes are recorded in each result JSON. Dependency artifacts were reused; no build runs concurrently with measurements.

`run_bench.py` accepts `--before`, `--after`, `--before-cwd`, `--after-cwd`, `--filter`, and `--output`. Filters are `version_groups`, `include_snapshots`, and `lazy_client`. Three separate before/after repetitions alternate execution order. Each case warms up for 100 ms and samples at least 500 ms; reported times include creation and destruction of each result, but exclude fixture setup. Raw per-repetition test output is preserved beside the structured JSON. The final summary uses the median of the three per-repetition mean operation times.

Grouping uses 10 and 100 ungrouped package diffs, with either default rules or `^(feat|feature|enhancement)$` and `^(breaking|major)$` custom regexes. The separate `grouped_custom_rules_control` verifies real grouped version calculation and rejection of an invalid grouped regex. All four existing update-config tests also pass. Ungrouped packages filtered by `release_commits` no longer validate an unused custom regex during the group scan; active candidates and grouped packages still do.

Snapshot cases use 10 or 100 packages with 100 short conventional commit messages each, plus 100 packages with zero commits. Inclusion settings are none, one source, and every source. The dense-empty case costs an additional ~10.5 µs; dense nonempty snapshots are effectively unchanged. Absent/sparse inclusion avoids most allocations. These are isolated preparation measurements, not an end-to-end speedup claim.

Client cases use a dummy token and `http://127.0.0.1:9` as the API base; no requests are made. They test absent/plain templates and 0/100 commits while reusing the returned diff vector. `remote_client_control` checks that a remote template still obtains a client, and missing tokens still fail. A synthetic dependency-update commit skips all network requests in this control. The existing updater tests also pass.

`bench_authenticated_update.py` adapts the offline CLI fixture from benchmark PR #3146: one unchanged package, identical local released sources, explicit plain changelog template, fixed release date, disabled semver checks, dummy token, loopback repository URL. Setup/reset and per-run output validation are outside timing. Three alternating repetitions each have one warmup and 10 measured updates. The measured candidate CLI is built from the clean production commit after removing all instrumentation, using `--release --locked --no-default-features --features all-static`.
