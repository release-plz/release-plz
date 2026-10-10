#!/usr/bin/env bash
set -euo pipefail
source /workspace/.devtools/activate.sh
export CARGO_TARGET_DIR=/workspace/release-plz/target
cd /workspace/perf-update
exec 9>/workspace/perf-build.lock
flock 9
EVIDENCE=/workspace/perf-evidence/update
cp "$EVIDENCE/original-Cargo.lock" Cargo.lock
cp "$EVIDENCE/inheritance.rs" crates/release_plz/benches/inheritance.rs
python3 "$EVIDENCE/extract_inheritance.py" crates/release_plz_core/src/command/update/updater.rs crates/release_plz/benches/inheritance_source.rs
python3 - <<'PY'
from pathlib import Path
p=Path('crates/release_plz/Cargo.toml')
s=p.read_text()
if 'name = "inheritance"' not in s:
    s=s.replace('[package.metadata.binstall]', '[[bench]]\nname = "inheritance"\nharness = false\n\n[package.metadata.binstall]')
p.write_text(s)
PY
cargo metadata --format-version 1 > "$EVIDENCE/metadata.json" 2> "$EVIDENCE/resolve-minimal.log"
python3 - <<'PY'
import tomllib
from pathlib import Path
old=tomllib.loads(Path('/workspace/perf-evidence/update/original-Cargo.lock').read_text())['package']
new=tomllib.loads(Path('Cargo.lock').read_text())['package']
old_versions={(p['name'],p['version'],p.get('source')) for p in old}
new_versions={(p['name'],p['version'],p.get('source')) for p in new}
assert old_versions <= new_versions, old_versions-new_versions
Path('/workspace/perf-evidence/update/lock-versions-check.txt').write_text('All original package versions and sources retained.\n')
PY
cargo bench --locked --no-default-features --features all-static -p release-plz --bench update --bench inheritance --no-run > "$EVIDENCE/build-main-locked.log" 2>&1
cp "$CARGO_TARGET_DIR/release/release-plz" "$EVIDENCE/release-plz-before"
cp crates/release_plz/benches/inheritance_source.rs "$EVIDENCE/inheritance-source-before.rs"
cargo bench --locked --no-default-features --features all-static -p release-plz --bench inheritance -- --save-baseline inheritance-before > "$EVIDENCE/inheritance-before.log" 2>&1
cargo bench --locked --no-default-features --features all-static -p release-plz --bench update -- 'workspace_10_(noop|one_change)' --save-baseline inheritance-before > "$EVIDENCE/cli-before.log" 2>&1
cargo bench --locked --no-default-features --features all-static -p release-plz --bench update -- --test > "$EVIDENCE/cli-smoke-before.log" 2>&1
for count in 10 100; do
  python3 "$EVIDENCE/cli_fixture.py" "$EVIDENCE/fixture-$count" "$count"
  (
    cd "$EVIDENCE/fixture-$count/current"
    CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$EVIDENCE/fixture-$count/target" RELEASE_PLZ_NO_ANSI=1 RELEASE_PLZ_LOG=error strace -f -c -o "$EVIDENCE/strace-before-$count.txt" "$EVIDENCE/release-plz-before" update --release-date 2025-01-01 --changelog-config cliff.toml --registry-manifest-path "$EVIDENCE/fixture-$count/released/Cargo.toml"
    CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$EVIDENCE/fixture-$count/target" RELEASE_PLZ_NO_ANSI=1 RELEASE_PLZ_LOG=error strace -e trace=openat -o "$EVIDENCE/opens-before-$count.txt" "$EVIDENCE/release-plz-before" update --release-date 2025-01-01 --changelog-config cliff.toml --registry-manifest-path "$EVIDENCE/fixture-$count/released/Cargo.toml"
  )
done
git apply "$EVIDENCE/inheritance.patch"
python3 "$EVIDENCE/extract_inheritance.py" crates/release_plz_core/src/command/update/updater.rs crates/release_plz/benches/inheritance_source.rs
cargo bench --locked --no-default-features --features all-static -p release-plz --bench update --bench inheritance --no-run > "$EVIDENCE/build-after.log" 2>&1
cp "$CARGO_TARGET_DIR/release/release-plz" "$EVIDENCE/release-plz-after"
cp crates/release_plz/benches/inheritance_source.rs "$EVIDENCE/inheritance-source-after.rs"
cargo bench --locked --no-default-features --features all-static -p release-plz --bench inheritance -- --baseline inheritance-before > "$EVIDENCE/inheritance-after.log" 2>&1
cargo bench --locked --no-default-features --features all-static -p release-plz --bench update -- 'workspace_10_(noop|one_change)' --baseline inheritance-before > "$EVIDENCE/cli-after.log" 2>&1
cargo bench --locked --no-default-features --features all-static -p release-plz --bench update -- --test > "$EVIDENCE/cli-smoke-after.log" 2>&1
for count in 10 100; do
  (
    cd "$EVIDENCE/fixture-$count/current"
    CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$EVIDENCE/fixture-$count/target" RELEASE_PLZ_NO_ANSI=1 RELEASE_PLZ_LOG=error strace -f -c -o "$EVIDENCE/strace-after-$count.txt" "$EVIDENCE/release-plz-after" update --release-date 2025-01-01 --changelog-config cliff.toml --registry-manifest-path "$EVIDENCE/fixture-$count/released/Cargo.toml"
    CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$EVIDENCE/fixture-$count/target" RELEASE_PLZ_NO_ANSI=1 RELEASE_PLZ_LOG=error strace -e trace=openat -o "$EVIDENCE/opens-after-$count.txt" "$EVIDENCE/release-plz-after" update --release-date 2025-01-01 --changelog-config cliff.toml --registry-manifest-path "$EVIDENCE/fixture-$count/released/Cargo.toml"
    git diff --exit-code
  )
done
