Baseline main: 74b20485
Production candidates: 6fe7088f (manifest dependency filter), 1272347b (propagation dependency filter).
Both are independent main-based commits; benchmark harness/exports are not in either commit.

Harness starts from PR #3146 commit 517963a958d3cf693ac83f270e3e648a4b4949d2.
reproduction-harness.patch contains all benchmark-only changes, including Cargo.lock
and a temporary public module export for the private propagation trait.

manifest_updates/many_local_dependencies/{10,100} uses 10 or 100 workspace members,
with N-1 root [workspace.dependencies] entries and N-1 direct dependencies in the
consumer. One provider alternates between 1.0.1 and 1.0.2. It verifies that both
references to that provider change, while all other requirements remain 1.0.0.
Package updates occur in timed body. Fixture creation + Cargo metadata are excluded.
The existing PR #3146 manifest_updates/independent benchmarks remain in this file.

dependency_propagation/many_updated_dependencies/{10,100} uses 10 or 100 renamed
local providers, all changing from 1.0.0 to 1.0.1, plus one consumer. Half of its
dependencies inherit workspace templates; half are target-specific build deps.
Every provider should release the consumer. It checks returned length and names.
Timed body is dependencies_to_update itself, including its manifest read/parse,
repeated path resolution and version requirement comparisons. Git history/Cargo
metadata/setup are excluded.

Portable reproduction (EVIDENCE points to the evidence directory in a local checkout):
EVIDENCE=/absolute/path/to/evidence/manifest
git switch --detach 74b20485
git apply "$EVIDENCE/reproduction-harness.patch"
# Use a clean target directory; serialize baseline/after runs on an idle machine.
cargo bench --locked -p release-plz --no-default-features --features all-static --bench manifest_updates --bench dependency_propagation -- --save-baseline manifest-main
# Candidate 1:
git apply "$EVIDENCE/manifest-name-filter.patch"
cargo bench --locked -p release-plz --no-default-features --features all-static --bench manifest_updates -- --baseline manifest-main
git apply -R "$EVIDENCE/manifest-name-filter.patch"
# Candidate 2 is independently compared against main:
git apply "$EVIDENCE/propagation-name-filter.patch"
cargo bench --locked -p release-plz --no-default-features --features all-static --bench dependency_propagation -- --baseline manifest-main
git apply -R "$EVIDENCE/propagation-name-filter.patch"

The task's actual orchestration script (run-comparison.sh) additionally acquires
flock /workspace/perf-results/cargo.lock, uses one shared Cargo target directory,
and captures logs. Its initial cargo clean --release -p release_plz_core prevents
an artifact from another local clone being reused with stale source timestamps.

Individual correctness validation: existing set_version integration tests for
first candidate; package_dependencies unit test + dependency_updates_match_renamed_
packages_among_unrelated_updates integration test for second candidate. Root runs
broader suite on combined changes.

Final outcome: both candidates improve the focused workload. Manifest comparisons
were run twice because the initial independent/10 control was noisy. The repeat
found no statistically significant control changes and reproduced the focused
wins. No third run was needed. Propagation ran once with tight intervals.
Summary files store Criterion slope estimates and 95% confidence intervals.
