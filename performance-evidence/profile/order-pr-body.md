Computing release order scans all selected workspace packages for every dependency, including dependencies outside the workspace. Build a borrowed name index once and use it for these lookups. Input traversal, cycle detection, and first-match behavior for duplicate names stay the same. Zero/one-package inputs return immediately because there is no inter-package ordering work.

Measured the production function on dependency chains with 20 external dependencies per package, supplied in reverse dependency order. Every case verifies the full returned order. Three serialized before/after rounds (two seconds per case), reversing order in round two:

| Workspace packages | Before, µs (three runs) | After, µs (three runs) | Median change |
| --- | --- | --- | --- |
| 1 | 0.106, 0.109, 0.109 | 0.052, 0.063, 0.052 | **51.8% faster** (0.109 → 0.052 µs) |
| 10 | 2.948, 3.513, 3.415 | 4.557, 4.607, 5.546 | **34.9% slower** (3.415 → 4.607 µs) |
| 100 | 192.154, 196.047, 192.394 | 83.270, 68.939, 67.121 | **64.2% faster** (192.394 → 68.939 µs) |
| 500 | 11,237.505, 11,135.461, 11,526.378 | 898.416, 1,065.606, 1,080.787 | **90.5% faster** (11.238 → 1.066 ms) |

This targets larger workspaces. Hashing adds **1.192 µs** to the median 10-package case; the single-package fast path avoids that setup cost. Measurements cover release-order calculation on Linux / Xeon Platinum 8573C / Rust 1.99.0.

Validation: **8 tests passed** with `cargo test --locked --release -p release_plz_core release_order::tests --lib`, covering dependency order, cycles, development dependencies, and feature-enabled dependencies.

[Reproducible harness, exact source snapshots, raw results and environment](https://github.com/release-plz/release-plz/tree/codex/performance-evidence-20261008/performance-evidence/profile) are on a separate evidence branch. This PR contains only the production change.
