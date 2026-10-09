import glob, pathlib, re, subprocess, sys
root=pathlib.Path(__file__).resolve().parent
deps='/workspace/perf-main/target/release/deps'
recorded = dict(re.findall(r'--extern (\w+)=(\S+)', (root/'build.log').read_text()))
externs=['anyhow','cargo_metadata','chrono','git_cliff_core','parse_changelog','regex','serde','serde_json','tracing','fs_err']
args=['rustc','--edition=2024','-C','opt-level=3','-C','debuginfo=0','-L',f'dependency={deps}']
if '--test' in sys.argv:
    args+=['--test']
    externs+=['expect_test']
    target=root/'unit-tests'
else:
    target=root/'benchmark'
for name in externs:
    libs=[recorded[name]] if name in recorded else glob.glob(f'{deps}/lib{name}-*.rlib')
    if len(libs)!=1: raise RuntimeError(f'Ambiguous dependency {name}: {libs}')
    args+=['--extern',f'{name}={libs[0]}']
args += [str(root/'bench.rs'),'-o',str(target)]
print(' '.join(args),flush=True)
subprocess.run(args,check=True)
