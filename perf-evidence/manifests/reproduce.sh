#!/usr/bin/env bash
# Reproduce the two measured manifest optimizations without editing a user's checkout.
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
    printf 'Usage: %s /path/to/release-plz [new-output-directory]\n' "$0" >&2
    exit 2
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repository="$(git -C "$1" rev-parse --show-toplevel)"
output_directory="${2:-$PWD/manifest-perf-results}"
if [[ -e "$output_directory" ]]; then
    printf 'Output path already exists; choose a new directory: %s\n' "$output_directory" >&2
    exit 2
fi

candidates=(baseline dependency-scan borrow-updates)
revisions=(
    267b5775ee1d52e9d22460fc5b65babe4f681f52
    a13cac0183e03f246156e6d2a93431b533ecd439
    b0a13f030d0c483bf7463515d685402085f296ba
)
for revision in "${revisions[@]}"; do
    if ! git -C "$repository" cat-file -e "${revision}^{commit}"; then
        printf 'Missing commit %s. Fetch it before running this script.\n' "$revision" >&2
        printf 'For example: git -C %q fetch origin %q\n' "$repository" "$revision" >&2
        exit 2
    fi
done
for command in cargo rustc python3; do
    command -v "$command" >/dev/null || { printf 'Required command missing: %s\n' "$command" >&2; exit 2; }
done

mkdir -p -- "$output_directory"
output_directory="$(cd -- "$output_directory" && pwd)"
export CARGO_TARGET_DIR="$output_directory/target"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
export CARGO_INCREMENTAL=0
scratch_directory="$(mktemp -d "${TMPDIR:-/tmp}/release-plz-manifest-perf.XXXXXX")"
worktrees=()
cleanup() {
    for tree in "${worktrees[@]}"; do
        git -C "$repository" worktree remove --force -- "$tree" || true
    done
    rm -rf -- "$scratch_directory"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

rustc -Vv > "$output_directory/rustc-version.txt"
if command -v lscpu >/dev/null; then
    lscpu > "$output_directory/cpu.txt"
fi

for index in "${!candidates[@]}"; do
    candidate="${candidates[$index]}"
    tree="$scratch_directory/$candidate"
    git -C "$repository" worktree add --detach "$tree" "${revisions[$index]}"
    worktrees+=("$tree")
    git -C "$tree" rev-parse HEAD >> "$output_directory/revisions.txt"
    python3 - "$tree" "$script_dir" <<'PY'
import json
import pathlib
import sys
worktree, harness_directory = map(pathlib.Path, sys.argv[1:])
for relative_source, harness in [
    ('crates/release_plz_core/src/command/update/package_dependencies.rs', 'dependency_scan.rs'),
    ('crates/release_plz_core/src/command/update/mod.rs', 'update_manifests.rs'),
]:
    source = worktree / relative_source
    harness_path = harness_directory / harness
    assert harness_path.is_file(), harness_path
    with source.open('a') as stream:
        stream.write('\n#[cfg(test)]\ninclude!(' + json.dumps(str(harness_path), ensure_ascii=False) + ');\n')
PY
    (
        cd -- "$tree"
        # Only this run's target directory is cleaned; downloaded dependencies are shared.
        cargo clean --release -p release_plz_core
        cargo test --locked --release -p release_plz_core --lib --no-run --message-format=json \
            > "$output_directory/${candidate}-build.json" \
            2> "$output_directory/${candidate}-build.log"
    )
    python3 - "$candidate" "$output_directory" <<'PY'
import hashlib
import json
import pathlib
import shutil
import sys
candidate, output = sys.argv[1], pathlib.Path(sys.argv[2])
rows = (json.loads(line) for line in (output / f'{candidate}-build.json').read_text().splitlines() if line.startswith('{'))
executables = [row['executable'] for row in rows if row.get('reason') == 'compiler-artifact' and row.get('executable') and row['target']['name'] == 'release_plz_core']
assert len(executables) == 1, executables
binary = output / f'{candidate}-tests'
shutil.copy2(executables[0], binary)
with (output / 'binaries.txt').open('a') as stream:
    stream.write(f'{candidate} {hashlib.sha256(binary.read_bytes()).hexdigest()} {binary}\n')
PY
done

# No compilation runs concurrently with these measurements. Each before/after pair
# uses an independent optimized binary, and each pair is repeated three times.
for repetition in 1 2 3; do
    for change in dependency-scan borrow-updates; do
        if [[ "$change" == dependency-scan ]]; then
            filter=benchmark_dependency_scan
        else
            filter=benchmark_update_manifests
        fi
        for candidate in baseline "$change"; do
            log="$output_directory/${candidate}-${filter}-${repetition}.log"
            "$output_directory/${candidate}-tests" "$filter" --ignored --nocapture --test-threads=1 > "$log" 2>&1
            cat -- "$log"
        done
    done
done

"$output_directory/dependency-scan-tests" versionless_and_workspace_dependencies_to_update --nocapture \
    > "$output_directory/dependency-scan-tests.log" 2>&1
"$output_directory/borrow-updates-tests" command::update --nocapture \
    > "$output_directory/borrow-updates-tests.log" 2>&1
python3 "$script_dir/summarize.py" "$output_directory" > "$output_directory/results.md"
printf 'Results: %s/results.md\n' "$output_directory"
