History and package comparison performance investigation
========================================================

Base: release-plz/release-plz main 74b20485.
Repository clone: /workspace/perf-history.
Build target: /workspace/perf-coordinator/target.
Toolchain: the session's Rust stable tools activated by
  source /workspace/.devtools/activate.sh
Builds and measurements were serialized with:
  flock /workspace/perf-results/cargo.lock <command>

MEASURED CANDIDATE: PACKAGE FILE METADATA

Production branch perf/package-file-metadata, commit c85b7381.
The production commit modifies only package_compare.rs (implementation + regression
test); it contains no benchmark, Cargo.toml, or Cargo.lock changes.

The old loop queried is_symlink() and exists() for each ordinary file. The new
loop ignores generated/previously-compared manifests first and makes one
symlink_metadata() call for each remaining local file.

Portable benchmark source/patch: metadata-harness.patch and metadata_benchmark.rs.
It invokes the actual private are_packages_equal_cached function through an ignored
unit test. Both fixtures have valid Cargo manifests and Cargo.toml.orig, so the
extracted-package file listing is used. Cached lists must each contain N+2 files.
Every comparison must return true, proving that all N source files are visited.
Each source file has 152 bytes of valid Rust comments. The source file counts are
32 and 512, in addition to Cargo.toml/Cargo.toml.orig.

Each variant runs 30 warmup comparisons, then 31 samples of 50 comparisons.
Directory creation, file writes, and initial cache filling precede timing.
Results are medians of per-comparison times, not overall release-plz update gains.

Raw observations:
  metadata-before-bench.log
  metadata-after-bench.log
Compact results:
  metadata-summary.json
  32 files: 86.624620 -> 76.301660 microseconds, 11.9169% less time.
  512 files: 1374.763620 -> 1230.192040 microseconds, 10.5161% less time.

Reproduction on another checkout:
  1. Check out main 74b20485, apply metadata-harness.patch.
  2. Run:
     cargo test --locked --release -p release_plz_core --lib \
       package_compare::metadata_benchmark::cached_package_comparison \
       -- --ignored --exact --nocapture
  3. Remove the harness patch, cherry-pick c85b7381, reapply the harness patch,
     and repeat the same command. Keep third-party builds cached, but ensure
     release_plz_core itself recompiles for each variant.
  4. Parse the PERF_JSON records. Each contains all 31 seconds-per-call samples.

Exact session orchestration is in run-comparisons.sh and resume-comparisons.sh;
their filesystem paths describe this session. Saved release test executables are
metadata-before-tests and metadata-after-tests (session-local, omitted from this
evidence branch). Running them directly requires
unsetting CARGO_TARGET_DIR when executing existing package-comparison tests,
because an existing cargo-package test expects its archive beneath its own
temporary fixture's target directory. The initial baseline test failure from
that environment leak is retained in metadata-before-tests.log. After unsetting
the variable all 8 baseline tests and all 9 candidate tests passed; rerun results
are in metadata-before-tests-rerun.log and metadata-after-tests.log.

SHELVED CANDIDATE: REUSE THE CHECKED-OUT HISTORY TIP

Branch perf/reuse-head-snapshot, commit 36d96c21.
It avoids checking out HEAD again when the history walk visits the snapshot
already checked out. A head-only walk also avoids the final HEAD restoration.
This is NOT recommended for a PR: wall-clock improvements did not reproduce.

Workloads are unmodified single_noop and workspace_10_noop from PR #3146's
update benchmark. Each paired run uses the same saved baseline/candidate release
executables; all measurements are under the shared lock. Criterion uses 10 flat
samples, one-second warmup and a five-second requested measurement period.

Compact authoritative results: checkout-summary.json.
The individual run values are Criterion mean point estimates in milliseconds:

                     Before   After    Relative change
single_noop, run 1     78.956   81.159     +2.79%
single_noop, run 2     93.660   91.313     -2.51%
single_noop, run 3     97.043   92.473     -4.71%
workspace 10, run 1   521.226  482.168     -7.49%
workspace 10, run 2   600.479  603.590     +0.52%
workspace 10, run 3   589.699  605.112     +2.61%

Median of the three run estimates:
single_noop: 93.660 -> 91.313 ms (-2.51%), no reliable improvement.
workspace_10_noop: 589.699 -> 603.590 ms (+2.36%), no reliable improvement.
Only the first workspace pair was statistically significant; it did not repeat.
No fourth timing run was attempted.

Criterion samples/estimates:
  criterion/<case>/{history-before,new} (run 1)
  criterion-run-2/update/<case>/{history-before,new}
  criterion-run-3/update/<case>/{history-before,new}
Run 1's baseline text log was truncated during a script edit; its complete raw
Criterion sample and estimate JSON is preserved. Runs 2 and 3 have full text logs.

Separate untimed GIT_TRACE2_EVENT runs did establish the intended command saving:
workspace_10_noop ran 31 git checkout commands before and 11 after (20 fewer).
Trace files: checkout-before-git-trace.jsonl and checkout-after-git-trace.jsonl.
This subprocess count is not presented as a reproducible wall-clock improvement.
All 26 offline update integration tests passed for this candidate.
