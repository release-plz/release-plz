# Manifest performance evidence

This directory contains the measured harnesses, raw observations and reproduction script for two independent changes. Production PRs do not contain these benchmarks.

| Variant | Exact source commit |
|---|---|
| Baseline (`main` at measurement time) | `267b5775ee1d52e9d22460fc5b65babe4f681f52` |
| Filter dependency release checks by resolved package name | `a13cac0183e03f246156e6d2a93431b533ecd439` |
| Borrow planned updates while editing manifests | `b0a13f030d0c483bf7463515d685402085f296ba` |

Each candidate is based directly on the baseline. The measurements use separate optimized binaries, not a combination of the two changes. The matching production diffs are also retained in `dependency-scan.patch` and `borrow-updates.patch`.

## Reproduce from any directory

Prerequisites: Bash, Git, Python 3, Cargo/Rust, and the repository's normal native build dependencies. The original run used Rust 1.99.0 on Linux x86_64 with an AMD EPYC 9V45 processor; see `rustc-version.txt` and `cpu.txt`. Rust 1.99.0 can be selected with `RUSTUP_TOOLCHAIN=1.99.0` after installing it with rustup. The script otherwise uses the toolchain already configured in your shell. Cargo may download build dependencies.

Clone the source repository or use an existing local clone, then fetch the recorded commits if that clone does not contain them:

```bash
git clone https://github.com/release-plz/release-plz.git /tmp/release-plz-source
git -C /tmp/release-plz-source fetch origin \
  267b5775ee1d52e9d22460fc5b65babe4f681f52 \
  a13cac0183e03f246156e6d2a93431b533ecd439 \
  b0a13f030d0c483bf7463515d685402085f296ba
```

From this evidence directory, run:

```bash
RUSTUP_TOOLCHAIN=1.99.0 bash ./reproduce.sh \
  /tmp/release-plz-source /tmp/manifest-perf-results
```

Both arguments accept arbitrary filesystem paths, including paths containing spaces. The output directory must not already exist. You can invoke the script by its absolute path from another directory; the harness paths are resolved relative to the script, not relative to `/workspace` or the current directory.

The script creates three disposable detached worktrees at the exact revisions, installs both external `#[cfg(test)] include!(...)` harnesses in each, builds and copies the three optimized test binaries, and runs each before/after comparison three times. It then runs the relevant existing core tests and writes `results.md` and `results.json`. The temporary worktrees and their instrumentation are removed on exit, including on ordinary failures or interruption. The supplied source checkout's tracked files are never edited. All generated Cargo artifacts are confined to the new output directory; its `target` directory can be large.

Each build uses:

```bash
cargo test --locked --release -p release_plz_core --lib --no-run --message-format=json
```

The selected binary is copied before the next build. Each measured invocation uses `--ignored --nocapture --test-threads=1`; no compilation runs during measurement. Run this on an otherwise idle machine. This portable script reproduces the recorded procedure; it was syntax-checked after packaging, without launching another benchmark run.

## Workloads and interpretation

`dependency_scan.rs` extends the workspace layout from benchmark PR #3146. It invokes the real `Package::dependencies_to_update` implementation for a consumer with one renamed path dependency and 1, 10 or 100 updated packages, either including or excluding that dependency. The normal cases include manifest reads/parsing, canonicalization and version requirement checks. Each process warms up for 100 calls and measures 10,000 calls per case. Output is checked before and after timing. Additional controls use a package without dependencies and synthetic Cargo metadata containing 100 registry dependencies; those controls measure 1,000,000 calls and confirm that the existing no-path-dependency early return remains independent of the update count.

`update_manifests.rs` extends #3146's independent workspace fixtures with preconstructed update plans and invokes the real `update_manifests` function. It writes the manifests for 10 or 100 packages while retaining exactly 0, 65,536 or 524,288 changelog bytes per planned update. The harness asserts the text lengths. Two plans alternate versions 1.0.1 and 1.0.2 across 100 timed iterations after 10 warmup iterations. Assertions verify every resulting package version. Setup, metadata discovery and result assertions are outside timing.

These are measurements of the affected phases, including their real manifest I/O, not claims about total `release-plz update` runtime. `results.md` lists all three observations and arithmetic means; percentages compare those means. Very small control timings should be interpreted at their nanosecond scale, not as end-to-end effects.

## Retained evidence

- `*-benchmark_*-1.log`, `*-2.log`, `*-3.log`: the original three runs, including success assertions and iteration counts.
- `all-runs.log`: the concatenated original benchmark output.
- `results.md` and `results.json`: summarized original measurements.
- `dependency_scan.rs` and `update_manifests.rs`: the exact measured harness sources; verify from this directory with `sha256sum -c harness-sha256.txt`.
- `*-tests.log`: the existing dependency regression test and 19 core update tests that passed.
- `*-build.log`, `binaries.txt`, `rustc-version.txt`, `cpu.txt`: build and environment provenance. Binary paths in historical records refer to the original workspace; reproduction regenerates binaries at the supplied output path.
- `summarize.py`: regenerate summaries from existing raw logs with `python3 summarize.py /path/to/results-directory`.
- `*-pr-body.md`: draft PR descriptions containing all three before/after observations.

Large compiled binaries, target directories and Cargo JSON build streams need not be published. The unmeasured source-allocation experiment is excluded; only the two measured candidate harnesses are included.
