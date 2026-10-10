Changelog ownership benchmark evidence

Baseline: release-plz main 74b20485.
Changes are independent, each based directly on that main commit:
- move-release: 755c03e82eeb355da11e5fa92b8dd934f1bdb3ad
- move-commits: 3a0baa733af819e25aabac77504e68c62abf2023

Environment: x86_64 Linux, AMD EPYC 9V45 96-Core Processor,
rustc 1.99.0 (b940084d7 2026-09-28), optimized --release builds.
All builds and measurements were serialized with the other performance work.
Every baseline/candidate test executable was rebuilt (Cargo fresh=false) and
copied before the next candidate build replaced the shared target executable.

Fixtures:
- 100 or 1000 conventional fix commits with deterministic IDs and messages.
- Metadata absent, or author/committer names/emails and remote username/PR number
  populated. Contributor names cycle through 20 values.
- Fixed release date 2025-01-01.
- generate: public Changelog::generate with the default changelog template;
  ChangelogBuilder::build runs before the measured section.
- update: private updater::get_changelog including both full changelog and new
  release body generation. Its simple custom template follows benchmark PR #3146.
  Fixture creation and cloning the input ChangelogRequest happen before timing.
- Output lengths and hashes match before/after for every fixture and timing pass.

Latency:
benchmark-harness.patch adds two ignored unit tests, without dependency changes.
Five warmup invocations, then 25 batches of at least 100 ms of measured work.
The log reports each batch mean in microseconds and the mean of those means.
Runs 1 and 3 measure before then after; run 2 reverses order. There are exactly
three full latency passes. Raw logs are *-run[123].log. Measurements include the
real functions' work and output disposal. They are not whole-CLI measurements.
No allocation counter is installed in these binaries.

Allocation volume:
allocation-harness.patch is an alternative to the latency patch, not applied on
top of it. It wraps std::alloc::System only in a separate test executable and
counts successful alloc, alloc_zeroed and realloc calls during the same measured
functions. Requested bytes are the cumulative sum of layout sizes/new realloc
sizes, not peak live heap usage or RSS. It warms each fixture five times, then
checks that all 25 observations give identical allocation/byte counts. Allocation
runs are not used to claim elapsed-time improvements. Raw logs are
*-allocations.log.

Reproduction in a clean disposable checkout, once at 74b20485 and once at each
candidate revision (replace /path/to/evidence with this evidence directory):

  git apply /path/to/evidence/benchmark-harness.patch
  cargo test --locked --release -p release_plz_core --lib perf_changelog -- --ignored --nocapture --test-threads=1
  git restore crates/release_plz_core/src/command/update/updater.rs

  git apply /path/to/evidence/allocation-harness.patch
  cargo test --locked --release -p release_plz_core --lib perf_changelog -- --ignored --nocapture --test-threads=1
  git restore crates/release_plz_core/src/command/update/updater.rs

The public generate benchmark is the relevant measurement for move-release;
updater get_changelog is the relevant one for move-commits. The complete local
orchestration is in run-suite.sh, run-repeats.sh and run-allocations.sh; these use
local branch names and a shared target and are not needed for portable reruns.
All production commits exclude both benchmark patches and all evidence files.

Correctness:
Existing core tests selected by `changelog`, excluding local performance tests:
18 passed on main and on each independent change. All three timing runs produced
identical output hashes for each before/after fixture. The production changes
only transfer ownership earlier and preserve processing/rendering behavior.
Move release
| Fixture | Allocation calls, before → after | Cumulative allocated bytes, before → after |
| --- | ---: | ---: |
| 100 commits | 36,585 → 36,283 (−0.83%) | 4,720,247 → 4,652,752 (−1.43%) |
| 100 commits + metadata | 51,285 → 50,483 (−1.56%) | 5,532,097 → 5,455,752 (−1.38%) |
| 1,000 commits | 357,894 → 354,892 (−0.84%) | 45,086,463 → 44,410,568 (−1.50%) |
| 1,000 commits + metadata | 504,894 → 496,892 (−1.58%) | 53,204,963 → 52,440,568 (−1.44%) |

Move commits
| Fixture | Allocation calls, before → after | Cumulative allocated bytes, before → after |
| --- | ---: | ---: |
| 100 commits | 42,319 → 42,118 (−0.47%) | 5,317,407 → 5,250,417 (−1.26%) |
| 100 commits + metadata | 61,919 → 61,218 (−1.13%) | 6,154,557 → 6,078,717 (−1.23%) |
| 1,000 commits | 405,937 → 403,936 (−0.49%) | 49,480,695 → 48,809,805 (−1.36%) |
| 1,000 commits + metadata | 601,937 → 594,936 (−1.16%) | 57,852,195 → 57,092,805 (−1.31%) |
move-release
  run 1 generate/100/metadata_false: 1683.975 -> 1686.254 us (-0.14% faster), stdev 2.8%/4.4%
  run 1 generate/100/metadata_true: 2295.755 -> 2309.312 us (-0.59% faster), stdev 2.1%/6.2%
  run 1 generate/1000/metadata_false: 17094.612 -> 17325.539 us (-1.35% faster), stdev 5.3%/9.1%
  run 1 generate/1000/metadata_true: 25109.088 -> 24346.892 us (3.04% faster), stdev 7.7%/8.4%
  run 2 generate/100/metadata_false: 1790.607 -> 1656.587 us (7.48% faster), stdev 9.8%/1.0%
  run 2 generate/100/metadata_true: 2322.173 -> 2237.134 us (3.66% faster), stdev 2.4%/1.5%
  run 2 generate/1000/metadata_false: 19467.801 -> 15800.598 us (18.84% faster), stdev 15.0%/3.9%
  run 2 generate/1000/metadata_true: 24622.711 -> 23933.846 us (2.80% faster), stdev 10.3%/3.8%
  run 3 generate/100/metadata_false: 1674.537 -> 1701.124 us (-1.59% faster), stdev 1.4%/5.6%
  run 3 generate/100/metadata_true: 2291.261 -> 2252.882 us (1.68% faster), stdev 1.1%/1.0%
  run 3 generate/1000/metadata_false: 16390.624 -> 16257.653 us (0.81% faster), stdev 4.4%/7.2%
  run 3 generate/1000/metadata_true: 23687.047 -> 23892.826 us (-0.87% faster), stdev 3.6%/14.0%
move-commits
  run 1 update/100/metadata_false: 2249.162 -> 2269.344 us (-0.90% faster), stdev 2.0%/10.3%
  run 1 update/100/metadata_true: 3113.647 -> 3077.827 us (1.15% faster), stdev 1.7%/1.9%
  run 1 update/1000/metadata_false: 22205.542 -> 21925.383 us (1.26% faster), stdev 2.0%/2.2%
  run 1 update/1000/metadata_true: 32397.275 -> 31244.545 us (3.56% faster), stdev 5.9%/4.3%
  run 2 update/100/metadata_false: 2284.688 -> 2262.257 us (0.98% faster), stdev 3.3%/3.5%
  run 2 update/100/metadata_true: 3135.108 -> 3057.545 us (2.47% faster), stdev 2.1%/1.3%
  run 2 update/1000/metadata_false: 21747.445 -> 21355.112 us (1.80% faster), stdev 4.0%/1.0%
  run 2 update/1000/metadata_true: 31235.963 -> 32028.263 us (-2.54% faster), stdev 2.3%/4.7%
  run 3 update/100/metadata_false: 2277.795 -> 2247.618 us (1.32% faster), stdev 3.2%/1.6%
  run 3 update/100/metadata_true: 3127.992 -> 3044.724 us (2.66% faster), stdev 1.8%/1.5%
  run 3 update/1000/metadata_false: 21882.766 -> 21465.638 us (1.91% faster), stdev 1.5%/1.7%
  run 3 update/1000/metadata_true: 33504.209 -> 32096.717 us (4.20% faster), stdev 12.2%/5.1%
