# Changelog header benchmark

This standalone Criterion harness compiles a copy of the actual production
`changelog_parser.rs` from each measured revision. It measures `parse_header`,
not the entire update command. Input creation, assertions and lazy-regex
initialization happen before timing. Each call includes constructing the returned
header string.

The `short` case has one release and a short introductory paragraph;
`long_header` has an approximately 4 KiB introduction and one release;
`history_100` has a short introduction and 100 releases. All three lack an
Unreleased section. `unreleased_control` exercises the unchanged first regex.

Reproduce from the repository with this directory copied outside its workspace:

```sh
BASE=8d3c2269
AFTER=codex/perf-changelog-header-match
EVIDENCE=/absolute/path/to/changelog
git show "$BASE":crates/release_plz_core/src/changelog_parser.rs > "$EVIDENCE/src/parser.rs"
cargo bench --locked --manifest-path "$EVIDENCE/Cargo.toml" --bench header -- --save-baseline before
git show "$AFTER":crates/release_plz_core/src/changelog_parser.rs > "$EVIDENCE/src/parser.rs"
cargo bench --locked --manifest-path "$EVIDENCE/Cargo.toml" --bench header -- --baseline before
cargo test --locked --manifest-path "$EVIDENCE/Cargo.toml"
```

The saved Cargo.lock keeps dependencies identical between variants. Measurements
use the optimized bench profile, Criterion 0.8.2, 100 samples, 1 second of warmup
and a 3-second measurement target. Builds and benchmark runs share an exclusive
lock with the other investigations to avoid competing workloads. Host: Linux
x86_64, five available AMD EPYC 9V45 vCPUs, Rust 1.99.0.
