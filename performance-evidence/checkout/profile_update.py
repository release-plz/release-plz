"""Standalone adaptation of PR #3143's validated update fixtures; never shipped."""
import argparse, json, os, pathlib, shutil, subprocess, time, tomllib
p = argparse.ArgumentParser()
p.add_argument('--binary', required=True)
p.add_argument('--root', required=True)
p.add_argument('--packages', type=int, default=10)
p.add_argument('--commits', type=int, default=0)
p.add_argument('--inherited', action='store_true')
p.add_argument('--prepare', action='store_true')
p.add_argument('--trace', choices=['summary','commands'])
p.add_argument('--log', default='error')
p.add_argument('--runs', type=int, default=1)
a=p.parse_args()
root=pathlib.Path(a.root).resolve()
env=os.environ.copy()
env.update(CARGO_NET_OFFLINE='true', CARGO_TARGET_DIR=str(root/'target'), RELEASE_PLZ_NO_ANSI='1', RELEASE_PLZ_LOG=a.log)
env.pop('GIT_TOKEN',None)
def run(cmd,cwd,**kwargs):
    return subprocess.run(cmd,cwd=cwd,env=env,check=True,capture_output=True,text=True,**kwargs)
current=root/'current'
released=root/'released'
if a.prepare:
    assert not root.exists(), root
    for ws in [current,released]:
        ws.mkdir(parents=True)
        manifest='[workspace]\nmembers = ["packages/*"]\nresolver = "3"\n'
        if a.inherited:
            manifest+='[workspace.package]\nversion = "1.0.0"\n'
        (ws/'Cargo.toml').write_text(manifest)
        for i in range(a.packages):
            package=ws/f'packages/package-{i}'
            (package/'src').mkdir(parents=True)
            version='version.workspace = true' if a.inherited else 'version = "1.0.0"'
            (package/'Cargo.toml').write_text(f'[package]\nname = "package-{i}"\n{version}\nedition = "2024"\n')
            (package/'src/lib.rs').write_text('// Initial release\n')
        run(['cargo','generate-lockfile','--offline'],ws)
    (current/'.gitattributes').write_text('* -text\n')
    (current/'release-plz.toml').write_text('[workspace]\nsemver_check = false\n')
    (current/'cliff.toml').write_text('''[changelog]
header = "# Changelog\\n"
body = """
## [{{ version }}] - {{ timestamp | date(format="%Y-%m-%d") }}
{% for commit in commits %}- {{ commit.message }}
{% endfor %}
"""
[git]
conventional_commits = true
filter_unconventional = false
''')
    run(['git','init'],current)
    run(['git','config','user.name','Performance fixture'],current)
    run(['git','config','user.email','benchmark@example.invalid'],current)
    run(['git','config','commit.gpgsign','false'],current)
    run(['git','add','.'],current)
    run(['git','commit','-m','chore: initial release'],current)
    src='// Initial release\n'
    for i in range(a.commits):
        src+=f'// Fix {i}\n'
        (current/'packages/package-0/src/lib.rs').write_text(src)
        run(['git','add','.'],current)
        run(['git','commit','-m',f'fix: change {i}'],current)
for iteration in range(a.runs):
    run(['git','reset','--hard','HEAD'],current)
    run(['git','clean','-fd'],current)
    cmd=[a.binary,'update','--release-date','2025-01-01','--changelog-config','cliff.toml','--registry-manifest-path',str(released/'Cargo.toml')]
    if a.trace:
        trace_file=root/f'{a.trace}-{iteration}.txt'
        options=['-f','-c'] if a.trace=='summary' else ['-f','-tt','-T','-e','trace=process']
        cmd=['strace',*options,'-o',str(trace_file),*cmd]
    start=time.perf_counter()
    result=run(cmd,current)
    elapsed=time.perf_counter()-start
    (root/f'output-{iteration}.txt').write_text(result.stdout+result.stderr)
    workspace=tomllib.loads((current/'Cargo.toml').read_text())
    for i in range(a.packages):
        package=current/f'packages/package-{i}'
        manifest=tomllib.loads((package/'Cargo.toml').read_text())
        changed=a.commits>0 and (i==0 or a.inherited)
        expected='1.0.1' if changed else '1.0.0'
        version=workspace['workspace']['package']['version'] if a.inherited else manifest['package']['version']
        assert version==expected,(i,version,expected)
        changelog=package/'CHANGELOG.md'
        assert changelog.exists()==changed,(i,changed)
        if changed:
            contents=changelog.read_text()
            assert '1.0.1' in contents
            if i==0:
                for commit in range(a.commits):
                    assert f'change {commit}\n' in contents
    if a.commits==0:
        assert run(['git','status','--porcelain'],current).stdout==''
    print(json.dumps(dict(iteration=iteration,packages=a.packages,commits=a.commits,inherited=a.inherited,seconds=elapsed,trace=a.trace)))
