When a Git token is configured, `fill_commits` currently builds a reqwest Git client even if the changelog template only uses local commit fields. Construct it only when the existing required-information check requests remote usernames or PR numbers. Remote templates keep the same client and missing-token behavior.

Measured the actual optimized `fill_commits` method with 100 commits, a plain commit-message template, and a dummy token. No network requests occur. Three independent before/after repetitions alternate order, each with 100 ms warmup and at least 500 ms sampling; fixture setup is outside timing.

| Run | Before | After |
| --- | ---: | ---: |
| 1 | 427.978 µs | 0.569 µs |
| 2 | 438.082 µs | 0.578 µs |
| 3 | 408.102 µs | 0.566 µs |

**Median: 427.978 µs → 0.569 µs, a 99.867% reduction in this metadata-filling stage.** With zero commits and the same template, 403.988 µs → 0.185 µs. This removes unnecessary client construction; it does not change remote requests.

Also ran the offline single-package no-op CLI fixture from benchmark PR #3146, adding a dummy token and explicit loopback repository URL. Each of three repetitions has one warmup and 10 validated `update` runs from separate clean production binaries:

| Run | Before | After |
| --- | ---: | ---: |
| 1 | 93.917 ms | 90.799 ms |
| 2 | 92.396 ms | 95.083 ms |
| 3 | 94.609 ms | 91.000 ms |

The overall mean was 93.641 → 92.294 ms (1.44% lower), but the second repetition was slower, so this does **not** establish a consistent end-to-end speedup. The isolated client-construction reduction is repeatable in all three repetitions. Both binaries execute the same 16 Git commands and leave the no-op fixture clean.

Validation: existing updater unit tests pass, and a remote-template control verifies both successful client provision and the existing missing-token error without external network access. The CLI validates manifests, changelog absence, and repository cleanliness after every sample. Benchmark code is excluded from this PR.
