Manifest updates only need to read each planned package and next version, but currently clone the entire update plan before separating inherited and independent versions. This copies package metadata, full changelogs and other release text solely to edit manifests.

Partition references to the planned updates and pass those references to `set_version`. Remove the now-unnecessary private `update_versions` wrapper and the now-unused public `PackagesUpdate::updates_clone` method; `updates()` remains available for callers that explicitly need to clone.

Benchmark evidence (three before/after runs, release profile, Rust 1.99.0, Linux x86_64; base `267b5775`, change `b0a13f03`):

| Workload | Before: runs 1 / 2 / 3 (µs) | After: runs 1 / 2 / 3 (µs) | Mean before → after (µs) | Less time |
|---|---|---|---|---|
| 10 packages; 0 KiB changelog per package | 117.106 / 114.239 / 112.286 | 110.373 / 107.284 / 106.833 | 114.544 → 108.163 | 5.6% |
| 100 packages; 0 KiB changelog per package | 1,513.190 / 1,526.820 / 1,517.650 | 1,513.596 / 1,512.695 / 1,501.719 | 1,519.220 → 1,509.337 | 0.7% |
| 10 packages; 64 KiB changelog per package | 163.197 / 163.064 / 162.488 | 107.182 / 106.215 / 106.350 | 162.916 → 106.582 | 34.6% |
| 100 packages; 64 KiB changelog per package | 2,522.905 / 2,481.800 / 2,459.974 | 1,480.600 / 1,461.992 / 1,490.069 | 2,488.226 → 1,477.554 | 40.6% |
| 10 packages; 512 KiB changelog per package | 834.321 / 803.879 / 800.595 | 109.338 / 109.168 / 111.299 | 812.932 → 109.935 | 86.5% |
| 100 packages; 512 KiB changelog per package | 11,474.952 / 11,518.416 / 11,701.953 | 1,486.959 / 1,474.481 / 1,469.538 | 11,565.107 → 1,476.993 | 87.2% |

The primary 64 KiB case improves the actual manifest-update phase by **34.6% for 10 packages and 40.6% for 100 packages**. The zero-length changelog control is effectively unchanged for 100 packages; 512 KiB shows how avoiding copies scales with longer changelog histories.

A temporary harness extends #3146's independent-workspace fixtures with a `PackagesUpdate` plan and invokes the actual `update_manifests` function. Each planned package carries exactly 0/65,536/524,288 changelog bytes (asserted by the harness). Two preconstructed plans alternate versions 1.0.1 and 1.0.2 across 100 iterations, including actual manifest parsing and writes. Fixture creation and assertions that every package ends at 1.0.2 are outside timing. Measurements are for this phase, not total `release-plz update` runtime. No benchmark code is included in this PR.

Validation: all 19 existing core update-module tests pass; benchmark assertions pass in every run.
