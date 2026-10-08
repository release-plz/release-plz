#!/usr/bin/env python3
"""Build isolated production revisions, then run three paired template benchmarks."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

repo = Path(sys.argv[1]).resolve()
output = Path(sys.argv[2]).resolve()
output.mkdir(parents=True, exist_ok=True)
baseline = '6e42d3c2'
changed = 'e9aa63cb'
harness = Path(__file__).with_name('bench.rs').resolve()
env = os.environ.copy()
env['CARGO_TARGET_DIR'] = str(output / 'target')
binaries = {}
worktrees = {}
with tempfile.TemporaryDirectory(prefix='release-template-benchmark-') as temporary:
    try:
        for name, revision in [('before', baseline), ('after', changed)]:
            tree = Path(temporary) / name
            subprocess.run(['git', '-C', str(repo), 'worktree', 'add', '--detach', str(tree), revision], check=True)
            worktrees[name] = tree
            source = tree / 'crates/release_plz_core/src/project.rs'
            with source.open('a') as file:
                file.write('\ninclude!(' + __import__('json').dumps(str(harness)) + ');\n')
            command = ['cargo', 'test', '--release', '--locked', '-p', 'release_plz_core', '--lib', '--no-run', '--message-format=json']
            result = subprocess.run(command, cwd=tree, env=env, text=True, stdout=subprocess.PIPE, check=True)
            import json, shutil
            artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
            executable = next(artifact['executable'] for artifact in artifacts if artifact.get('reason') == 'compiler-artifact' and artifact.get('executable') and artifact['target']['name'] == 'release_plz_core')
            binaries[name] = output / ('core-tests-' + name)
            shutil.copy2(executable, binaries[name])
        for run in range(1, 4):
            for name in ['before', 'after']:
                with (output / f'run-{run}-{name}.log').open('w') as log:
                    subprocess.run([str(binaries[name]), 'benchmark_release_templates', '--ignored', '--nocapture', '--test-threads=1'], cwd=worktrees[name] / 'crates/release_plz_core', env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    finally:
        for tree in worktrees.values():
            subprocess.run(['git', '-C', str(repo), 'worktree', 'remove', '--force', str(tree)], check=True)
