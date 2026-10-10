# Default 0.0.x next-version calculation

Base: `8d3c2269f0eab776abe53ba11a7fa2f6ac35360a` on release-plz main.

The `next_version` group is the unchanged Criterion benchmark from PR #3143, the
base of #3146. The `zero_zero` group uses exactly the same borrowed fix, feature,
and breaking histories (10, 100, and 1,000 commits), starting from `0.0.1` and
asserting `0.0.2`. Histories are constructed and expected results checked outside
the timing loop. Each iteration black-boxes the current version and history and
calculates the next version. These are library-call timings, not end-to-end CLI
measurements.

Criterion 0.8.2, 40 samples, 0.5-second warmup, 1-second measurement target per
case, optimized bench profile. Baseline and candidate use the same harness lock
file, compiler, build settings, and machine. Builds and measurements are
serialized with `/workspace/perf-build.lock`.

Environment: Linux x86_64, five available AMD EPYC 9V45 vCPUs; Rust 1.99.0
(b940084d7 2026-09-28), Cargo 1.99.0 (5f94df478 2026-08-27).

The standalone evidence crate points its `next_version` dependency at the
checkout under test. To reproduce elsewhere, adjust that dependency path in
Cargo.toml, and the Criterion output directory in `benches/next_version.rs`.

```sh
cargo bench --locked --bench next_version -- --save-baseline before
# Apply only the production change to the checkout under test.
cargo bench --locked --bench next_version -- --baseline before
```

No benchmark sources, manifests, lock files, or generated outputs belong in the
production PR.

Measured candidate commit: `1f833e83` (`perf: skip commit parsing for default 0.0.x increments`).

Initial results (Criterion slope point estimates; `summary-run1.json` contains
all 18 cases):

| 0.0.x history | Before | After | Less time |
| --- | ---: | ---: | ---: |
| 10 fix commits | 1.95545 µs | 11.806 ns | 99.396% |
| 100 fix commits | 19.6989 µs | 12.080 ns | 99.939% |
| 1,000 fix commits | 196.429 µs | 11.932 ns | 99.994% |
| 1,000 feature commits | 195.995 µs | 11.825 ns | 99.994% |
| 1,000 breaking commits | 170.531 µs | 12.002 ns | 99.993% |

The improvement is expected to scale with history length: the candidate only
checks whether the borrowed history is empty. The original 1.0.0 histories still
use the existing parser. Their initial slope estimates varied from 2.33% faster
to 1.56% slower; these changes are not claimed as improvements. The feature/1000
control had a noisier 2.82% mean slowdown (0.53% slope slowdown), so that case was
repeated separately.

Validation: 40 next_version integration tests and 17 doctests passed;
`cargo clippy --locked -p next_version --all-targets -- -D warnings` and
`cargo fmt --all -- --check` passed. Regression coverage includes empty 0.0.x
histories, no-increment exclusions, and 0.0.x prereleases; existing tests cover
custom major/minor regexes and both forced-increment flags.

Reproduction layout: the published evidence lives at
`performance-evidence/next-version` in its evidence branch. Copy that directory
outside the measured checkout before switching the checkout between the base
and candidate commits. Replace the harness's machine-specific dependency path
(`/workspace/perf-next-version/crates/next_version`) with the absolute path to
`crates/next_version` in that checkout. Replace the `output_directory` argument
with a writable results directory. Run both commands from the copied harness
directory so they share its committed Cargo.lock. The explicit `[workspace]`
keeps this harness separate from the release-plz workspace.

The selected 1.0.0 feature/1000 control was run three times in total. The third
pair reversed execution order (candidate first, baseline second). Its slope
estimate was 194.872 µs before and 194.906 µs after (+0.017%); Criterion detected
no significant difference (`p = 0.83`). The candidate's mean was 0.106% faster in
that reversed pair. No stable-version performance improvement is claimed.

| Control run | Before slope | After slope | Candidate mean change |
| --- | ---: | ---: | ---: |
| Initial | 194.624 µs | 195.651 µs | +2.815% |
| Repeat | 194.603 µs | 198.211 µs | +1.040% |
| Reversed order | 194.872 µs | 194.906 µs | -0.106% |

`summary-run1.json` records all original point estimates; `summary-run2.json` and
`summary-run3.json` retain the selected repeated control. Raw candidate samples
from the full initial run are preserved in each case's `after-r1` directory;
selected repeat samples are in `before-r2`, `after-r2`, `candidate-r3`, and
`before-r3`. The final Criterion `new` directory for that control contains the
third run's baseline because the execution order was reversed.
