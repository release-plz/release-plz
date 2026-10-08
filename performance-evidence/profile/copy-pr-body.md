`update` and `release-pr` copy a repository before modifying it. The tracked-file fallback currently checks each source file and its ancestors even when the directory walker has already copied that file. Check the destination first for ordinary tracked paths; keep Gitlink recursion and the existing fallback for missing destinations.

Measured the production copy function on clean repositories with nested Rust source files, with two warmups and 20 copies per run. Each copy checks every tracked file's contents, Git status, and exclusion of ignored build output after timing. Three serialized before/after rounds, reversing order in round two:

| Tracked Rust files | Before, ms (three runs) | After, ms (three runs) | Median reduction |
| --- | --- | --- | --- |
| 100 | 4.656, 4.727, 4.917 | 4.163, 4.026, 3.926 | **14.8%** (4.727 → 4.026 ms) |
| 1,000 | 26.023, 21.556, 26.036 | 14.472, 14.696, 17.259 | **43.5%** (26.023 → 14.696 ms) |

A separate `strace` run counted **43,117 → 25,114** metadata syscalls across three copies including identical validation, confirming the eliminated filesystem work. Timings above were collected without tracing, on Linux / Xeon Platinum 8573C / Rust 1.99.0.

Validation: **15 tests passed** with `cargo test --locked --release -p release_plz_core copy_dir::tests --lib`, covering ignored tracked files, deletions, symlinks, submodules, and split/sparse indexes.

[Reproducible harness, exact source snapshots, raw results and environment](https://github.com/release-plz/release-plz/tree/codex/performance-evidence-20261008/performance-evidence/profile) are on a separate evidence branch. This PR contains only the production change.
