Investigation only: no PR was opened for this candidate because the three runs did not establish a repeatable end-to-end improvement above the observed variability. The measurements and source change are retained for review.

During an update history walk, package equality fills the current snapshot's file-list cache and restores `Cargo.lock`. The subsequent changed-file check reuses that list but still performs another `git checkout Cargo.lock`. Skip that second restoration on cache hits; cache misses retain the existing restoration and error handling.

Measured the original `update/history_100` benchmark from #3146: one package, 100 fix commits, and `release-plz update` against a local released-package fixture. Before is main at `6e42d3c2`; after is this independent change at `3252cbc2`.

| Run | Before | After | Latency reduction |
| --- | ---: | ---: | ---: |
| 1 | 3.7216 s | 3.7184 s | 0.09% |
| 2 | 4.1355 s | 3.8118 s | 7.83% |
| 3 | 4.0478 s | 3.9176 s | 3.22% |

The average of the three mean estimates changed from **3.9683 s to 3.8159 s** (**3.84% lower observed latency**). The reductions ranged from 0.09% to 7.83%; the first pair is indistinguishable from noise, and only the second pair has separated 95% mean confidence intervals. These measurements do not establish a consistent size of end-to-end improvement.

Each round uses Criterion's mean estimate from 10 flat samples, a 1-second warmup, and a 5-second measurement target. Fixture resets and assertions are outside the timed command. Every iteration checks the resulting version and all 100 changelog entries. Measurements use optimized binaries on Linux, Rust 1.99.0, and an AMD EPYC 9V74 host with five virtual CPUs, without competing builds or benchmarks.

An additional successful `strace` validation shows `Cargo.lock` checkouts decreasing from **201 to 101**, while the 101 historical checkouts, 102 `cargo package --list` calls, and 100 commit-message lookups remain unchanged.

Validation also passed on combined checkout `40a53df9`, containing all ten independent performance changes:

- `cargo test --locked --no-default-features --features all-static --workspace`: 328 passed, 83 ignored with Docker tests disabled.
- `cargo clippy --locked --all-targets --all-features --workspace -- -D warnings`.
- `cargo fmt --all -- --check`.

The individual branch's benchmark assertions passed separately. Benchmark sources, raw results, and reproduction instructions are kept in the separate performance-evidence branch; no benchmark changes are included in this PR.
