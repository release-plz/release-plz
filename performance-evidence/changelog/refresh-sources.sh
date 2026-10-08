#!/usr/bin/env bash
set -euo pipefail
# Optional: verify/recreate source copies from a release-plz checkout containing
# the baseline and both performance branches. Usage: ./refresh-sources.sh /path/to/release-plz
repo="${1:?Pass the release-plz repository path}"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
base=6e42d3c20d11a5aee4b046c0a7e4f35f7e15d506
regex=87f3ef0e1e0ba5053965bddf4f727d0641be2d89
header=0aaf0a9ed9babd85216c3e8daab4bfb9b4e2ff41
for harness in header regex; do
  mkdir -p "$script_dir/$harness-harness/src"
done
git -C "$repo" show "$base:crates/release_plz_core/src/changelog.rs" > "$script_dir/regex-harness/src/before.rs"
git -C "$repo" show "$regex:crates/release_plz_core/src/changelog.rs" > "$script_dir/regex-harness/src/after.rs"
git -C "$repo" show "$base:crates/release_plz_core/src/changelog_parser.rs" > "$script_dir/regex-harness/src/changelog_parser.rs"
git -C "$repo" show "$base:crates/release_plz_core/src/changelog_parser.rs" > "$script_dir/header-harness/src/before.rs"
git -C "$repo" show "$header:crates/release_plz_core/src/changelog_parser.rs" > "$script_dir/header-harness/src/after.rs"
git -C "$repo" show "$base:crates/release_plz_core/CHANGELOG.md" > "$script_dir/header-harness/src/release_plz_core_CHANGELOG.md"
