When custom version-increment regexes are configured, the version-group scan compiles them for every package even when that package has no version group. Build the updater only after finding a group. Ungrouped release candidates still validate and use their rules when calculating their next version.

Measured the actual `get_version_groups` method in an optimized, temporary in-module benchmark: 100 ungrouped packages, ordinary custom minor/major regexes, 100 ms warmup and at least 500 ms sampling per case. Fixture setup is outside timing; the three independent before/after repetitions alternate order.

| Run | Before | After |
| --- | ---: | ---: |
| 1 | 9.217569 ms | 2.479031 µs |
| 2 | 9.969726 ms | 2.341283 µs |
| 3 | 9.153785 ms | 2.424468 µs |

**Median: 9.218 ms → 2.424 µs, a 99.974% reduction in this scan.** The 10-package case goes from 0.908 ms to 0.240 µs; the 100-package default-rules control goes from 1.569 µs to 0.532 µs. These are isolated stage measurements, not an end-to-end `update` speedup.

Validation: all four existing update-config tests pass, including invalid regexes, plus an actual grouped custom-rule check that verifies both the expected version and invalid-rule rejection. An invalid custom regex on an ungrouped package excluded by `release_commits` is no longer validated by this unrelated scan; active candidates and grouped packages still validate it.

The benchmark harness and raw results are separate evidence artifacts and are not part of this PR.
