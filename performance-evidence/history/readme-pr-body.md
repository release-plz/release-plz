Compare packaged contents before querying README metadata. A historical snapshot whose packaged files already differ can skip `cargo metadata`; equal or inconclusive file comparisons still check the README, including README files outside the package. Lockfile-restoration failures remain fatal, and a changed README can still establish inequality when package listing fails.

Observed `update/history_100` elapsed time fell **27.58%** (median of three independent Criterion mean estimates).

| #3146 scenario | Before: runs 1, 2, 3 (ms) | After: runs 1, 2, 3 (ms) | Median before → after (ms) | Time reduction |
| --- | --- | --- | --- | --- |
| `history_100` | 7119.16, 7169.13, 7084.98 | 5155.95, 5099.38, 5187.52 | 7119.16 → 5155.95 | 27.58% |
| `workspace_10_one_change` | 1389.13, 1458.87, 1351.81 | 1314.27, 1329.55, 1332.07 | 1389.13 → 1329.55 | 4.29% |

The workspace control varied more across runs than the observed median difference; no workspace speedup is claimed.

Measured against main `72465264` on Linux x86_64 with Rust 1.99.0 and optimized default-feature builds. The [PR #3146](https://github.com/release-plz/release-plz/pull/3146) update benchmarks used their unchanged settings (10 samples, 1 s warmup, 5 s measurement target; the history scenario automatically takes longer to collect all samples). Each variant ran three times, interleaved under an exclusive benchmark lock. Fixture setup/reset and version/changelog checks are outside the timer; every measured command runs offline and validates its result. Benchmark infrastructure is excluded from this PR.

Validation: `cargo test --locked --release -p release-plz --test all update::` — 22 update integration tests passed, including a new regression covering unchanged and changed external READMEs.

Raw logs, estimates, commands, and patches: [performance evidence](https://github.com/release-plz/release-plz/tree/codex/performance-evidence-20261008/history).
