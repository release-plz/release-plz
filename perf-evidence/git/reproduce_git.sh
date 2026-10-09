#!/usr/bin/env bash
# Usage: bash reproduce_git.sh REPOSITORY CANDIDATE_COMMIT skip-empty|share-head NEW_OUTPUT_DIRECTORY
# Requires Rust/Cargo, Git, Python >= 3.11, sha256sum, and flock on Linux.
set -euo pipefail
repo=$(realpath "$1")
revision=$2
candidate=$3
mkdir -p "$4"
output=$(realpath "$4")
evidence=$(cd "$(dirname "$0")" && pwd)
baseline=03a7393dc40f48bbaa6ceb776a4e95dfc04db627
export CARGO_TARGET_DIR="$output/target"
export CARGO_INCREMENTAL=0
exec 9>"$output/benchmark.lock"
flock 9
for side in before after; do
  source_dir="$output/$side-src"
  source_revision=$baseline
  if [ "$side" = after ]; then source_revision=$revision; fi
  git -C "$repo" worktree add --detach "$source_dir" "$source_revision"
  cp -R "$evidence/overlay/." "$source_dir/"
  (
    cd "$source_dir"
    # The shared Cargo target can otherwise retain artifacts from another worktree.
    # Only local crates are removed; registry dependencies are retained.
    cargo clean --release -p release-plz -p release_plz_core -p cargo_utils \
      -p git_cmd -p next_version -p fake_package -p test_logs
    cargo bench -p release-plz --bench update --bench manifest_updates --no-run
    cmp Cargo.lock "$evidence/overlay/Cargo.lock"
    cp "$CARGO_TARGET_DIR/release/release-plz" "$output/$side"
  ) >"$output/$side-build.log" 2>&1
  sha256sum "$output/$side"
done
case "$candidate" in
  skip-empty)
    python3 "$evidence/bench_git_update.py" --before "$output/before" --after "$output/after" \
      --packages 100 --tagged --expect-checkout-reduction 100 --output "$output/tagged100"
    python3 "$evidence/bench_git_update.py" --before "$output/before" --after "$output/after" \
      --packages 10 --tagged --expect-checkout-reduction 10 --output "$output/tagged10"
    ;;
  share-head)
    python3 "$evidence/bench_git_update.py" --before "$output/before" --after "$output/after" \
      --packages 100 --tagged --expect-head-read-reduction 99 --output "$output/tagged100"
    python3 "$evidence/bench_git_update.py" --before "$output/before" --after "$output/after" \
      --packages 10 --expect-head-read-reduction 9 --output "$output/untagged10"
    ;;
  *) echo 'candidate must be skip-empty or share-head' >&2; exit 2 ;;
esac
