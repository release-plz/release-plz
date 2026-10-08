# Changelog performance evidence

Base: release-plz/release-plz main `6e42d3c20d11a5aee4b046c0a7e4f35f7e15d506`.

Independent production commits:
- `87f3ef0e`: cache the PR-number regular expression in `default_git_config`.
- `0aaf0a9e`: use `Regex::find` when parsing the full Unreleased header match.

Each standalone harness contains exact copies of the applicable production module before and after the change. This runs both variants in one executable with the same dependencies and compiler settings. Before/after fixtures and generated outputs are compared for equality outside the timed region. The modules' existing unit tests are also compiled by `cargo test --release`. No harness or benchmark dependency is included in the production commits.

- `/workspace/changelog-regex-bench`: benchmarks `default_git_config(Some(PR_URL))` and building/generating one real changelog entry with a PR link. Asserts the generated markdown contains the correct link and that baseline/candidate output is identical.
- `/workspace/changelog-header-bench`: benchmarks parsing synthetic histories with 10, 100, and 1,000 releases plus this repository's actual 70,506-byte core changelog at the base revision. Checks parser equality for all fixtures and 384 combinations of heading case, spacing, Unicode, CRLF, and repeated Unreleased sections.

Criterion: 50 samples, 1 second warmup, 3 seconds measurement per benchmark. Run each executable three times while no other builds/benchmarks run. These are focused operation measurements; they do not claim an equal percentage improvement for a whole `release-plz update` invocation.

Commands (source `/workspace/.devtools/activate.sh` first):

```sh
CARGO_TARGET_DIR=/workspace/changelog-bench-target cargo bench --manifest-path /workspace/changelog-header-bench/Cargo.toml --locked --offline --bench headers --no-run
CARGO_TARGET_DIR=/workspace/changelog-bench-target cargo bench --manifest-path /workspace/changelog-regex-bench/Cargo.toml --locked --offline --bench pr_regex --no-run
CARGO_TARGET_DIR=/workspace/changelog-bench-target cargo test --manifest-path /workspace/changelog-header-bench/Cargo.toml --locked --offline --release
CARGO_TARGET_DIR=/workspace/changelog-bench-target cargo test --manifest-path /workspace/changelog-regex-bench/Cargo.toml --locked --offline --release
```

The initial build omits `--locked` only to prune the copied repository lockfile for each standalone manifest; subsequent commands use the retained lockfiles. Rust dependencies use the repository versions and default-feature choices, including git-cliff-core 2.14.2 with default features disabled.

Portable reproduction: `./run-benchmarks.sh` builds/tests `header-harness/` and `regex-harness/`, then runs each benchmark three times into `results/`. The source copies and lockfiles are included; no release-plz checkout is needed. Optionally run `./refresh-sources.sh /path/to/release-plz` to recreate the production source copies directly from the cited git revisions before reproducing. Use an otherwise idle machine for the timing phase. Compiler used for the recorded runs: `rustc 1.99.0 (b940084d7 2026-09-28)` (release profile, opt-level 3).

Run `python3 summarize.py` to recalculate the PR tables from raw `results/**/new/estimates.json` files. The arithmetic mean of each variant's three Criterion mean estimates is used for the overall time reduction. See `summary.json` for full-precision values.
