#!/usr/bin/env bash
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$script_dir/target}"
mkdir -p logs
cp snapshots/before.rs src/changelog.rs
cargo metadata --offline --no-deps --format-version 1 > logs/metadata.json
cargo run --locked --release --example semantics > logs/semantics-before.txt 2> logs/semantics-before-build.log
cargo run --locked --release --example allocations > logs/allocations-before.log 2>&1
cargo bench --locked --bench git_defaults -- --save-baseline git-defaults-before > logs/before.log 2>&1
cp snapshots/after.rs src/changelog.rs
cargo run --locked --release --example semantics > logs/semantics-after.txt 2> logs/semantics-after-build.log
cmp logs/semantics-before.txt logs/semantics-after.txt
cargo run --locked --release --example allocations > logs/allocations-after.log 2>&1
cargo bench --locked --bench git_defaults -- --baseline git-defaults-before > logs/after.log 2>&1
python3 - <<'COPY'
from pathlib import Path
import shutil, os
root = Path(os.environ['CARGO_TARGET_DIR']) / 'criterion/git_defaults'
for path in root.rglob('*.json'):
    if path.name in {'estimates.json', 'sample.json', 'benchmark.json'}:
        dest = Path('results') / path.relative_to(root)
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, dest)
COPY
cargo test --release --locked > logs/tests.log 2>&1
