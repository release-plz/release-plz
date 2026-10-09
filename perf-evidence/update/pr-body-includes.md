Preparing changelog includes currently deep-clones every package’s commit history. Snapshot only the source packages named in `changelog_include`, while retaining the original histories before any includes are applied. The regression test checks direct inclusion, duplicate requests, and that includes do not become transitive.

Measured the snapshot-preparation block verbatim, temporarily factored into a helper in an optimized in-module benchmark. Each of 100 packages has 100 realistic short commit messages; one package includes one source history. Three independent before/after repetitions alternate order, with 100 ms warmup and at least 500 ms sampling per case.

| Run | Before | After |
| --- | ---: | ---: |
| 1 | 932.899 µs | 9.840 µs |
| 2 | 991.153 µs | 9.956 µs |
| 3 | 966.588 µs | 10.637 µs |

**Median: 966.588 µs → 9.956 µs, a 98.97% reduction in snapshot preparation.** With no includes, the same histories take 986.737 µs → 0.637 µs (99.94% less); 100 packages with no commits and no includes take 5.838 µs → 0.625 µs.

Controls where every source history is included:

| Run | 100 × 100 commits: before → after | 100 × 0 commits: before → after |
| --- | ---: | ---: |
| 1 | 931.432 → 933.891 µs | 5.973 → 16.232 µs |
| 2 | 946.511 → 945.319 µs | 5.829 → 16.562 µs |
| 3 | 978.124 → 953.054 µs | 5.835 → 16.369 µs |

Including every nonempty history is effectively unchanged (median 946.511 → 945.319 µs). Including all 100 empty histories costs an extra **10.535 µs** (5.835 → 16.369 µs): collecting the requested source names adds overhead when there is nothing to clone. This change targets absent/sparse includes; it does not improve every workload.

These are isolated stage measurements, not an end-to-end `update` claim. Benchmark code and raw results remain outside this PR.
