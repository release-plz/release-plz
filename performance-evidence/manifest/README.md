# Manifest and workspace metadata benchmarks

The workspace lookup change is commit `46ddf488` (`perf: find workspace packages without collecting every member`), measured against `main` at `8d3c2269`. It searches package names before checking workspace membership, avoiding the temporary vector of every workspace member and unrelated membership checks.

The benchmark is separate from the production change. `manifest_perf.rs` contains the local Criterion harness, and `benchmark-overlay.patch` adds only its development dependencies, target registration, and corresponding lockfile entries. The production commit contains no benchmark files or dependency changes.

## Reproduce

Use an otherwise idle machine and the same optimized build configuration for both runs. Set `evidence_dir` to this directory's absolute path, then use a disposable checkout:

```sh
evidence_dir=/absolute/path/to/manifest
git clone https://github.com/release-plz/release-plz.git release-plz-lookup-bench
cd release-plz-lookup-bench
git checkout --detach 8d3c2269
git apply "$evidence_dir/benchmark-overlay.patch"
mkdir -p crates/cargo_utils/benches
cp "$evidence_dir/manifest_perf.rs" crates/cargo_utils/benches/manifest_perf.rs

cargo bench --locked -p cargo_utils --bench manifest_perf -- \
  workspace_package --save-baseline before

# Fetch the production PR branch first if this commit is not present locally.
git cherry-pick 46ddf488
cargo bench --locked -p cargo_utils --bench manifest_perf -- \
  workspace_package --baseline before
```

The uncommitted benchmark overlay does not overlap with the production commit. No rejected cache or dependency-table scripts are needed for this comparison.

The fixture first creates a workspace using the same temporary-workspace pattern as benchmark PR #3146, then loads its Cargo metadata. It appends ten synthetic nonmember packages per workspace member, plus an earlier nonmember with the requested name. It measures first-member, last-member, and absent-name lookups in metadata containing 10 members/101 nonmembers and 100 members/1001 nonmembers. Setup, Cargo subprocesses, and semantic assertions are outside the timer. Each timed lookup black-boxes its metadata, requested name, and result.

## Recorded results

Rust 1.99.0, Criterion 0.8.2, Linux x86_64 on AMD EPYC 9V45; 100 samples, one-second warmup, and three-second measurement per case. A shared lock excluded concurrent builds and other benchmarks throughout the before/after pair.

These values are Criterion **slope point estimates**, with reductions calculated directly from those estimates:

| Members | Lookup | Before | After | Less time |
| ---: | --- | ---: | ---: | ---: |
| 10 | First | 447.080 ns | 7.976 ns | 98.22% |
| 10 | Last | 471.059 ns | 36.613 ns | 92.23% |
| 10 | Missing | 499.711 ns | 77.356 ns | 84.52% |
| 100 | First | 32.323 µs | 29.121 ns | 99.91% |
| 100 | Last | 32.044 µs | 354.665 ns | 98.89% |
| 100 | Missing | 32.280 µs | 508.399 ns | 98.43% |

These are isolated lookup timings; command-level gains depend on the number of lookups. One paired run was sufficient for these large, consistent improvements.

Raw output is in `lookup-baseline.log` and `lookup-lazy.log`. Criterion estimates and samples are in `results/workspace_package/<case>/<count>/<before|after>/`, and exact numerical summaries are in `results/lookup-summary.json`.

## Validation and rejected experiments

The seven `cargo_utils` tests passed. The production commit adds `workspace_metadata::workspace_package_ignores_nonmembers_with_the_same_name` under `crates/release_plz/tests/all/`; it copies the existing `set-version-in-workspace` fixture and checks name collisions, nonmembers, missing names, and removed workspace membership. The root task will run it with the full workspace validation before publishing.

A dependency-path canonicalization cache improved shared-dependency fixtures but made a 100-member unique-dependency chain about 12% slower in two paired runs, so it was rejected. A lazy mutable dependency-table iterator also regressed small manifests and was rejected. Those exploratory sources/logs are retained only as investigation evidence.

## Mutable dependency tables: independent single-buffer change

Commit `c152f165` (`perf: collect mutable dependency tables into one buffer`) changes only `crates/cargo_utils/src/local_manifest.rs`. It can be cherry-picked directly onto `8d3c2269` without the lookup change. The current command caller consumes every dependency table. One shared vector removes the separate vectors previously allocated for package, workspace, and target sections.

Use a second disposable checkout at `8d3c2269`, apply the same `benchmark-overlay.patch`, and copy `manifest_perf.rs` to `crates/cargo_utils/benches/manifest_perf.rs` as above. Then run:

```sh
cargo bench --locked -p cargo_utils --bench manifest_perf -- \
  dependency_tables_mut --save-baseline before
# Fetch the production PR branch first if this commit is not present locally.
git cherry-pick c152f165
cargo bench --locked -p cargo_utils --bench manifest_perf -- \
  dependency_tables_mut --baseline before
```

The fixture parses a manifest before timing and traverses all mutable dependency tables, black-boxing each returned reference. It contains three package dependency tables and one workspace table, plus 0/10/100 target sections with three dependency tables each. Untimed assertions mutate every returned table and verify the changes using immutable traversal.

| Target sections | Dependency tables | Before | After | Less time |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 4 | 60.761 ns | 38.957 ns | 35.88% |
| 10 | 34 | 491.767 ns | 435.058 ns | 11.53% |
| 100 | 304 | 3.678 µs | 3.095 µs | 15.85% |

These are slope point estimates with the same toolchain, Criterion parameters, and isolated-run policy as the lookup benchmark. Raw output is `tables-baseline2.log` and `tables-buffered.log`; samples and estimates are in `results/dependency_tables_mut/`, with exact summaries in `results/tables-summary.json`. This was one paired measurement for the final single-buffer variant. The separate, rejected lazy-chain implementation is unrelated to this accepted variant.

The returned iterator now collects all table references at construction. Current in-repository code consumes them all; external callers stopping early may do more work. Invalid workspace/target scalars still panic, at construction rather than first iteration. Existing `set_version` integration tests exercise valid manifests with ordinary, renamed, inherited, target-specific, and relocated dependencies; the root task includes them in full workspace validation before publication.
