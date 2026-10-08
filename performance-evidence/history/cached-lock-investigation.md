Investigated but not proposed as a PR. The three history rounds all favored this change, but the final individual Criterion comparison reported p = 0.09 (no detected change), and the first comparison was within the practical noise threshold. Retained as local code and evidence only.

History package equality already caches Cargo’s file list and restores `Cargo.lock` for the current snapshot. Reusing that list while checking changed files now avoids a second `git checkout Cargo.lock`; uncached listings still restore the lockfile after running Cargo.

Observed `update/history_100` elapsed time fell **1.80%** (median of three independent Criterion mean estimates).

| #3146 scenario | Before: runs 1, 2, 3 (ms) | After: runs 1, 2, 3 (ms) | Median before → after (ms) | Time reduction |
| --- | --- | --- | --- | --- |
| `history_100` | 7119.16, 7169.13, 7084.98 | 6983.97, 7042.24, 6990.88 | 7119.16 → 6990.88 | 1.80% |
| `workspace_10_one_change` | 1389.13, 1458.87, 1351.81 | 1355.15, 1354.78, 1396.52 | 1389.13 → 1355.15 | 2.45% |

The workspace control varied more across runs than the observed median difference; no workspace speedup is claimed.

Measured against main `72465264` on Linux x86_64 with Rust 1.99.0 and optimized default-feature builds. The [PR #3146](https://github.com/release-plz/release-plz/pull/3146) update benchmarks used their unchanged settings (10 samples, 1 s warmup, 5 s measurement target; the history scenario automatically takes longer to collect all samples). Each variant ran three times, interleaved under an exclusive benchmark lock. Fixture setup/reset and version/changelog checks are outside the timer; every measured command runs offline and validates its result. Benchmark infrastructure is excluded from this PR.

Validation: `cargo test --locked --release -p release-plz --test all update::` — 21 existing update integration tests passed.

Raw logs, estimates, commands, and patches: [performance evidence](https://github.com/release-plz/release-plz/tree/codex/performance-evidence-20261008/history).
