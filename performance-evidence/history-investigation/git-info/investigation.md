Investigation only: no PR was opened for this candidate because the measured end-to-end improvement was inconsistent across the three runs. The measurements and source change are retained for review.

When a historical commit changes a packaged file, `release-plz update` currently launches one Git process to list changed files and another to read the commit message. Read both through one `git show` invocation, using a NUL separator between the message and file list. Preserve the existing conservative message-only fallback when changed-file inspection fails. The Git helper test covers root commits, multiline messages, and empty commits.

Measured a `history_100_root` variant of #3146's end-to-end update benchmark: one package at the repository root and 100 fix commits. Before is main at `6e42d3c2`; after is this independent change at `4c24388f`.

| Run | Before | After | Latency reduction |
| --- | ---: | ---: | ---: |
| 1 | 4.2667 s | 4.1480 s | 2.78% |
| 2 | 3.8999 s | 3.7894 s | 2.83% |
| 3 | 3.9338 s | 3.9760 s | -1.07% |

The average of the three mean estimates changed from **4.0335 s to 3.9711 s** (**1.55% lower observed latency**). However, the third round was **1.07% slower**, reversing the direction of the first two rounds. These measurements do not confirm a repeatable end-to-end performance win; this candidate remains unpublished despite its deterministic reduction in Git subprocesses.

The root layout exercises the changed-file intersection path: `Cargo.lock` physically exists alongside the package manifest. The original benchmark's nested workspace package lacks that package-local lockfile, so its file canonicalization falls back to including commits without running `git show`; it therefore does not exercise this optimization. The root variant excludes benchmark scaffolding from packaging, disables README inference, and additionally checks that the initial release commit is absent from the changelog.

Each round uses the original Criterion settings: 10 flat samples, a 1-second warmup, and a 5-second measurement target. Fixture resets and correctness assertions are outside the timed command. Every iteration checks the resulting version and all 100 changelog entries. Measurements use optimized binaries on Linux, Rust 1.99.0, and an AMD EPYC 9V74 host with five virtual CPUs, without competing builds or benchmarks.

Successful `strace` validations show **100 file-list commands plus 100 message commands becoming 100 combined commands**, removing 100 Git subprocesses. Both versions retain 201 lockfile checkouts and 102 `cargo package --list` calls.

Validation also passed on combined checkout `40a53df9`, containing all ten independent performance changes:

- `cargo test --locked --no-default-features --features all-static --workspace`: 328 passed, 83 ignored with Docker tests disabled.
- `cargo clippy --locked --all-targets --all-features --workspace -- -D warnings`.
- `cargo fmt --all -- --check`.

The individual before/after binaries passed the custom benchmark's assertions separately. Benchmark sources, raw results, and reproduction instructions are kept in the separate performance-evidence branch; no benchmark changes are included in this PR.
