When no custom git-cliff configuration is supplied, constructing the renderer currently creates the default configuration and then constructs another set of defaults to merge into it. Return the defaults directly, including the PR-link preprocessor, and retain the existing merge path for custom configurations.

A focused benchmark compiles the exact before/after production modules with the same optimized dependency artifacts. Three separate processes each use 21 alternating before/after measurement blocks (at least 100 ms each), after warmup. Times below are medians in microseconds:

| Run | Default config before → after (µs) | Component reduction | Build + generate before → after (µs) |
| --- | ---: | ---: | ---: |
| 1 | 2.305 → 1.154 | 49.95% | 143.000 → 142.907 |
| 2 | 2.337 → 1.189 | 49.11% | 143.929 → 143.137 |
| 3 | 2.304 → 1.176 | 48.95% | 145.608 → 144.858 |

The repeatable gain is approximately 49% in default configuration construction, saving about 1.1 µs per changelog. The 0.1–0.6% observed whole-render difference is too small to claim a meaningful generation speedup. A prepend case with PR-link formatting changes by 0.7–1.3%; no whole `update` speedup is claimed.

Every run checks identical serialized configuration both with and without PR links, plus byte-identical generated and prepended changelogs for both cases. All 32 existing-test executions pass across the three changelog source variants and the unchanged parser. The benchmark source and raw runs are kept separately from this PR, so it can merge independently of #3146.
