"""Profile the same offline 100-commit update shape as PR #3146."""
import collections
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

results = Path(__file__).resolve().parent
binary = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else results / "baseline-release-plz"
label = sys.argv[2] if len(sys.argv) > 2 else "update-history"

def run(args, cwd, **kwargs):
    return subprocess.run(args, cwd=cwd, check=True, capture_output=True, text=True, **kwargs)

with tempfile.TemporaryDirectory(prefix="release-plz-profile-") as temporary:
    root = Path(temporary)
    for name in ("current", "released"):
        project = root / name
        package = project / "packages/package-0"
        (package / "src").mkdir(parents=True)
        (project / "Cargo.toml").write_text('[workspace]\nmembers = ["packages/*"]\nresolver = "3"\n')
        (package / "Cargo.toml").write_text('[package]\nname = "package-0"\nversion = "1.0.0"\nedition = "2024"\n')
        (package / "src/lib.rs").write_text("// Initial release\n")
        run(["cargo", "generate-lockfile", "--offline"], project)
    current = root / "current"
    (current / ".gitattributes").write_text("* -text\n")
    (current / "release-plz.toml").write_text("[workspace]\nsemver_check = false\n")
    (current / "cliff.toml").write_text('''[changelog]
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
    run(["git", "init", "-q"], current)
    run(["git", "config", "user.name", "Benchmark"], current)
    run(["git", "config", "user.email", "benchmark@example.invalid"], current)
    run(["git", "add", "."], current)
    run(["git", "commit", "-qm", "Initial release"], current)
    source = "// Initial release\n"
    for index in range(100):
        source += f"// Fix {index}\n"
        (current / "packages/package-0/src/lib.rs").write_text(source)
        run(["git", "add", "."], current)
        run(["git", "commit", "-qm", f"fix: change {index}"], current)
    environment = os.environ.copy()
    environment.update(CARGO_NET_OFFLINE="true", CARGO_TARGET_DIR=str(root / "target"), RELEASE_PLZ_LOG="error", RELEASE_PLZ_NO_ANSI="1")
    environment.pop("GIT_TOKEN", None)
    trace = results / f"{label}-execve.log"
    output = run(["strace", "-f", "-e", "trace=execve", "-s", "4096", "-o", str(trace), str(binary), "update", "--release-date", "2025-01-01", "--changelog-config", "cliff.toml", "--registry-manifest-path", str(root / "released/Cargo.toml")], current, env=environment)
    assert 'version = "1.0.1"' in (current / "packages/package-0/Cargo.toml").read_text()
    changelog = (current / "packages/package-0/CHANGELOG.md").read_text()
    assert "Initial release" not in changelog
    for index in range(100):
        assert f"change {index}\n" in changelog
    calls = collections.Counter()
    cargo_pids = set()
    pending_execs = {}
    for line in trace.read_text().splitlines():
        pid = line.split()[0]
        if "execve(" in line and "<unfinished ...>" in line:
            pending_execs[pid] = line.replace("<unfinished ...>", "")
            continue
        if "<... execve resumed>" in line:
            line = pending_execs.pop(pid, "") + line.split("<... execve resumed>", 1)[1]
        if not line.endswith("= 0"):
            continue
        match = re.search(r'execve\("([^\"]+)", \[(.*?)\],', line)
        if not match:
            continue
        executable, arguments = match.groups()
        args = re.findall(r'"([^\"]*)"', arguments)
        if Path(executable).name == "cargo":
            # rustup's Cargo proxy execs the real Cargo in the same process.
            if pid in cargo_pids:
                continue
            cargo_pids.add(pid)
            calls["cargo " + " ".join(args[1:4])] += 1
        elif Path(executable).name == "git":
            git_args = args[1:]
            while git_args and git_args[0] in {"-C", "-c"}:
                git_args = git_args[2:]
            if git_args and git_args[0].startswith("--git-dir="):
                git_args = git_args[1:]
            calls["git " + " ".join(git_args[:2])] += 1
        else:
            calls[Path(executable).name] += 1
    (results / f"{label}-process-counts.json").write_text(json.dumps(calls, indent=2) + "\n")
    print(json.dumps(calls, indent=2))
