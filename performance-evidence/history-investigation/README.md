# Update-command profiling: candidates not published

These two changes each remove 100 Git subprocesses in a 100-commit fixture, but their three paired wall-clock measurements did not establish a consistent improvement. No production PR was opened for either candidate. Their source patches, harnesses, command counts, raw `strace` output and all Criterion samples are retained here for further investigation.

| Candidate | Run 1 reduction | Run 2 reduction | Run 3 reduction | Reduction across means |
| --- | ---: | ---: | ---: | ---: |
| Avoid redundant lockfile restoration | 0.09% | 7.83% | 3.22% | 3.84% |
| Read changed files and commit message together | 2.78% | 2.83% | -1.07% | 1.55% |

Positive percentages mean less elapsed time. The first candidate's results vary substantially, and the second is slower in its third pair. Neither should be treated as a confirmed end-to-end win. See each `summary.json` for full precision and confidence intervals.

The original nested-package fixture executes no changed-file `git show` calls: package-file canonicalization encounters a generated, absent package-local Cargo.lock and conservatively includes the commit. The root-package variant in `git-info/update_root.rs` exercises the changed-file intersection path. It retains the benchmark's sampling settings and output assertions, and additionally rejects the initial release commit from the changelog. See `git-info/NOTES.txt` for the fixture changes.

Untimed subprocess profiles show that `cargo package --list` still runs 102 times per update. Eliminating repeated Cargo invocations remains a larger potential improvement, but preserving historical package file selection requires more investigation than these small changes.

## Reproduction

Start from `main` commit `6e42d3c20d11a5aee4b046c0a7e4f35f7e15d506`. Apply `benchmark-harness.patch` in a disposable checkout to add the exact benchmark infrastructure used for these runs. It is derived from PR #3146 at `517963a958d3cf693ac83f270e3e648a4b4949d2`.

1. Build and save the baseline CLI with `cargo build --locked --release -p release-plz`.
2. Build the original harness with `cargo bench --locked -p release-plz --bench update --no-run` and save the executable reported by Cargo.
3. For the root variant, copy `git-info/update_root.rs` to `crates/release_plz/benches/update_root.rs`, append `[[bench]]`, `name = "update_root"`, and `harness = false` to `crates/release_plz/Cargo.toml`, then build with `cargo bench --locked -p release-plz --bench update_root --no-run`. Save this harness executable too.
4. In separate baseline checkouts, apply each candidate's `production.patch`, build its release CLI, and save that executable. Each patch is independent. If sharing a Cargo target directory across checkouts, touch changed production source files before building and verify that Cargo recompiles the intended crate; otherwise stale artifact reuse can invalidate the comparison.
5. Run the following command once for each candidate, replacing the paths with your saved executables and output directory:

```sh
python3 run-history-benchmarks.py \
  --before /absolute/path/to/saved-baseline-cli \
  --after /absolute/path/to/saved-candidate-cli \
  --harness /absolute/path/to/saved-benchmark \
  --embedded-cli /absolute/path/to/harness-target/release/release-plz \
  --benchmark history_100 \
  --output /absolute/path/to/results
```

Use `history_100_root` and its saved harness for the combined Git-command candidate. The harness embeds Cargo's CLI path when compiled: `--embedded-cli` must identify that exact path. The runner overwrites this disposable build artifact with the saved before/after binaries. Keep the saved binaries elsewhere. It runs three pairs sequentially, reverses the order in round two, and stores each run's raw samples in a distinct directory. Do not run other builds or benchmarks concurrently.

All measurements use successful update commands and validate the resulting version and all 100 changelog entries outside the clock. `strace` was used only for separate subprocess-count validation, never for reported timings. The observed machine, build logs, binary hashes and exact invocation protocol are preserved alongside the results.
