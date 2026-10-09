#!/usr/bin/env bash
set -euo pipefail
source /workspace/.devtools/activate.sh
worktree=$1
label=$2
cd "$worktree"
export CARGO_TARGET_DIR=/workspace/perf-main/target
export CARGO_NET_OFFLINE=true
cargo clean --release -p release-plz -p release_plz_core -p cargo_utils -p git_cmd -p next_version -p fake_package -p test_logs
cargo test --release --locked -p release_plz_core --lib --no-run --message-format=json > "/workspace/perf-evidence/update/build-$label.jsonl" 2> "/workspace/perf-evidence/update/build-$label.stderr"
python3 - "$label" <<'PY'
from pathlib import Path
import hashlib
import json
import shutil
import sys
label = sys.argv[1]
root = Path('/workspace/perf-evidence/update')
messages = [json.loads(line) for line in (root / f'build-{label}.jsonl').read_text().splitlines() if line.startswith('{')]
executables = [m['executable'] for m in messages if m.get('reason') == 'compiler-artifact' and m.get('executable') and m['target']['name'] == 'release_plz_core']
assert len(executables) == 1, executables
binary = root / f'bench-{label}'
shutil.copy2(executables[0], binary)
print(json.dumps({'source': executables[0], 'binary': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}))
PY
