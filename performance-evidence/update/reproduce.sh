#!/usr/bin/env bash
# Usage: ./reproduce.sh /path/to/release-plz /path/to/new-comparison-directory
# The source repository is read-only; all edits occur in a new local clone.
set -euo pipefail
SOURCE_REPOSITORY=$(realpath "$1")
COMPARISON_DIRECTORY=$(realpath -m "$2")
EVIDENCE_DIRECTORY=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
[[ ! -e "$COMPARISON_DIRECTORY" ]]
mkdir -p "$COMPARISON_DIRECTORY"
git clone --no-checkout "$SOURCE_REPOSITORY" "$COMPARISON_DIRECTORY/checkout"
cd "$COMPARISON_DIRECTORY/checkout"
git checkout --detach 8d3c2269f0eab776abe53ba11a7fa2f6ac35360a
export CARGO_TARGET_DIR="$COMPARISON_DIRECTORY/target"
export CARGO_BUILD_JOBS=4
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
cp Cargo.lock "$COMPARISON_DIRECTORY/main-Cargo.lock"
python3 - <<'PY'
from pathlib import Path
p=Path('Cargo.toml')
s=p.read_text().replace('[workspace.dependencies]', '[workspace.dependencies]\ncriterion = { version = "0.8.2", default-features = false, features = ["cargo_bench_support", "html_reports", "plotters"] }')
p.write_text(s)
p=Path('crates/release_plz/Cargo.toml')
s=p.read_text().replace('[dev-dependencies]', '[dev-dependencies]\ncriterion.workspace = true')
for name in ['update', 'inheritance']:
    s += f'\n[[bench]]\nname = "{name}"\nharness = false\n'
p.write_text(s)
PY
mkdir -p crates/release_plz/benches
cp "$EVIDENCE_DIRECTORY/update.rs" crates/release_plz/benches/update.rs
cp "$EVIDENCE_DIRECTORY/inheritance.rs" crates/release_plz/benches/inheritance.rs
# Only additive benchmark dependencies are resolved. Do not regenerate the lockfile.
cargo metadata --format-version 1 > /dev/null
python3 - "$COMPARISON_DIRECTORY/main-Cargo.lock" <<'PY'
import sys, tomllib
from pathlib import Path
identities = lambda path: {(p['name'], p['version'], p.get('source')) for p in tomllib.loads(Path(path).read_text())['package']}
assert identities(sys.argv[1]) <= identities('Cargo.lock')
PY
python3 "$EVIDENCE_DIRECTORY/extract_inheritance.py" crates/release_plz_core/src/command/update/updater.rs crates/release_plz/benches/inheritance_source.rs
cargo bench --locked --no-default-features --features all-static -p release-plz --bench inheritance -- --save-baseline inheritance-before |& tee "$COMPARISON_DIRECTORY/inheritance-before.log"
cargo bench --locked --no-default-features --features all-static -p release-plz --bench update -- 'workspace_10_(noop|one_change)' --save-baseline inheritance-before |& tee "$COMPARISON_DIRECTORY/cli-before.log"
git apply "$EVIDENCE_DIRECTORY/inheritance.patch"
python3 "$EVIDENCE_DIRECTORY/extract_inheritance.py" crates/release_plz_core/src/command/update/updater.rs crates/release_plz/benches/inheritance_source.rs
cargo bench --locked --no-default-features --features all-static -p release-plz --bench inheritance -- --baseline inheritance-before |& tee "$COMPARISON_DIRECTORY/inheritance-after.log"
cargo bench --locked --no-default-features --features all-static -p release-plz --bench update -- 'workspace_10_(noop|one_change)' --baseline inheritance-before |& tee "$COMPARISON_DIRECTORY/cli-after.log"
cargo bench --locked --no-default-features --features all-static -p release-plz --bench update -- --test
