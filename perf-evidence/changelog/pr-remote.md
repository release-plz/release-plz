Preparing remote changelog context currently serializes the metadata twice: once to insert it into the renderer, and once to construct an error message even when insertion succeeds. Format that error context lazily using `Debug`; successful rendering avoids the extra serialization. Failure diagnostics retain the value, displayed as Rust debug output instead of JSON.

A focused benchmark compiles the exact before/after production modules with the same optimized dependency artifacts. Three separate processes each use 21 alternating before/after measurement blocks (at least 100 ms each), after warmup. Times below are medians in microseconds. The primary case has 100 contributors, with usernames and two PR numbers each:

| Run | Context before → after (µs) | Reduction | Build + generate before → after (µs) | Reduction |
| --- | ---: | ---: | ---: | ---: |
| 1 | 60.171 → 52.005 | 13.57% | 336.222 → 325.608 | 3.16% |
| 2 | 60.087 → 51.035 | 15.07% | 340.830 → 330.552 | 3.02% |
| 3 | 60.781 → 51.111 | 15.91% | 333.616 → 324.506 | 2.73% |

The 10-contributor control reduces context preparation by 28.3–29.5%; whole generation changes by 0.5–1.2%. At 1,000 contributors, context preparation improves 15.5–16.8% and whole generation improves 4.5–5.5%. These are local changelog rendering measurements; no whole `update` speedup is claimed.

Every run verifies identical rendered remote fields for 1, 100, and 1,000 contributors. All 32 existing-test executions pass across the three changelog source variants and the unchanged parser. The benchmark source and raw runs are kept separately from this PR, so it can merge independently of #3146.
