from pathlib import Path
import json
import subprocess
import sys

deps = Path(sys.argv[1]).resolve()
root = Path(__file__).parent
command = ['rustc', '--edition=2024', '-O', str(root / 'file_comparison_bench.rs'),
           '-L', f'dependency={deps}', '-o', sys.argv[2]]
artifacts = None
if len(sys.argv) > 3:
    artifacts = [json.loads(line) for line in Path(sys.argv[3]).read_text().splitlines()]
for crate in ['release_plz_core', 'cargo_metadata', 'tempfile', 'fs_err']:
    if artifacts is None:
        candidates = list(deps.glob(f'lib{crate}-*.rlib'))
    else:
        candidates = [Path(filename) for artifact in artifacts
                      if artifact.get('reason') == 'compiler-artifact'
                      and artifact['target']['name'] == crate
                      for filename in artifact['filenames'] if filename.endswith('.rlib')]
    if len(candidates) != 1:
        raise RuntimeError(f'Expected one {crate} artifact, found {candidates}')
    command.extend(['--extern', f'{crate}={candidates[0]}'])
subprocess.run(command, check=True)
