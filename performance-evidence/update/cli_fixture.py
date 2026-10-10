from pathlib import Path
import os, sys, subprocess, json, time

def command(args, root, **kw):
    return subprocess.run(args, cwd=root, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kw)

def fixture(root, count, commits=0):
    root=Path(root)
    for state in ['current','released']:
        base=root/state
        base.mkdir(parents=True,exist_ok=True)
        (base/'Cargo.toml').write_text('[workspace]\nmembers = ["packages/*"]\nresolver = "3"\n')
        for index in range(count):
            pkg=base/f'packages/package-{index}'
            (pkg/'src').mkdir(parents=True,exist_ok=True)
            (pkg/'Cargo.toml').write_text(f'[package]\nname = "package-{index}"\nversion = "1.0.0"\nedition = "2024"\n')
            (pkg/'src/lib.rs').write_text('// Initial release\n')
        command(['cargo','generate-lockfile','--offline'],base)
    current=root/'current'
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
    command(['git','init'],current)
    command(['git','config','user.name','Benchmark'],current)
    command(['git','config','user.email','benchmark@example.invalid'],current)
    command(['git','config','commit.gpgsign','false'],current)
    (current/'README.md').write_text('# my awesome project')
    command(['git','add','.'],current)
    command(['git','commit','-m','add README'],current)
    for index in range(commits):
        with (current/'packages/package-0/src/lib.rs').open('a') as f: f.write(f'// Fix {index}\n')
        command(['git','add','.'],current)
        command(['git','commit','-m',f'fix: change {index}'],current)
    return root

if __name__=='__main__':
    fixture(Path(sys.argv[1]),int(sys.argv[2]),int(sys.argv[3]) if len(sys.argv)>3 else 0)
