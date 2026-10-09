#!/usr/bin/env python3
"""PR3146's offline CLI update fixture, kept outside production branches."""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time
import tomllib


def command(args, cwd, env=None):
    return subprocess.run(args, cwd=cwd, env=env, check=True, capture_output=True, text=True)


def fixture(root, packages, commits):
    for name in ["current", "released"]:
        tree = root / name
        tree.mkdir()
        (tree / "Cargo.toml").write_text('[workspace]\nmembers = ["packages/*"]\nresolver = "3"\n')
        for index in range(packages):
            package = tree / f"packages/package-{index}"
            (package / "src").mkdir(parents=True)
            (package / "Cargo.toml").write_text(f'[package]\nname = "package-{index}"\nversion = "1.0.0"\nedition = "2024"\n')
            (package / "src/lib.rs").write_text("// Initial release\n")
        command(["cargo", "generate-lockfile", "--offline"], tree)
    current = root / "current"
    (current / ".gitattributes").write_text("* -text\n")
    (current / "README.md").write_text("# my awesome project")
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
    command(["git", "init"], current)
    for key, value in [("user.name", "author_name"), ("user.email", "author@example.com"), ("commit.gpgsign", "false")]:
        command(["git", "config", key, value], current)
    command(["git", "add", "."], current)
    command(["git", "commit", "-m", "add README"], current)
    source = "// Initial release\n"
    for index in range(commits):
        source += f"// Fix {index}\n"
        (current / "packages/package-0/src/lib.rs").write_text(source)
        command(["git", "add", "."], current)
        command(["git", "commit", "-m", f"fix: change {index}"], current)


def run(root, binary, packages, commits, trace=None):
    current = root / "current"
    command(["git", "reset", "--hard", "HEAD"], current)
    command(["git", "clean", "-fd"], current)
    env = dict(os.environ, CARGO_NET_OFFLINE="true", CARGO_TARGET_DIR=str(root / "target"), RELEASE_PLZ_NO_ANSI="1", RELEASE_PLZ_LOG="error")
    env.pop("GIT_TOKEN", None)
    args = [binary, "update", "--release-date", "2025-01-01", "--changelog-config", "cliff.toml", "--registry-manifest-path", str(root / "released/Cargo.toml")]
    if trace:
        args = ["strace", "-f", "-qq", "-tt", "-T", "-e", "trace=process", "-s", "2000", "-o", trace, *args]
    start = time.perf_counter()
    output = command(args, current, env)
    elapsed = time.perf_counter() - start
    for index in range(packages):
        package = current / f"packages/package-{index}"
        changed = commits > 0 and index == 0
        expected = "1.0.1" if changed else "1.0.0"
        assert tomllib.loads((package / "Cargo.toml").read_text())["package"]["version"] == expected
        changelog = package / "CHANGELOG.md"
        assert changelog.exists() == changed
        if changed:
            contents = changelog.read_text()
            assert "1.0.1" in contents
            for commit in range(commits):
                assert f"change {commit}\n" in contents
    if not commits:
        assert not command(["git", "status", "--porcelain"], current).stdout
    return elapsed


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary")
    parser.add_argument("--packages", type=int, default=10)
    parser.add_argument("--commits", type=int, default=0)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--trace")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="release-plz-profile-") as directory:
        root = Path(directory)
        fixture(root, args.packages, args.commits)
        if args.trace:
            print(json.dumps({"profile_seconds": run(root, args.binary, args.packages, args.commits, args.trace)}))
        else:
            run(root, args.binary, args.packages, args.commits)
            timings = [run(root, args.binary, args.packages, args.commits) for _ in range(args.runs)]
            print(json.dumps({"binary": args.binary, "packages": args.packages, "commits": args.commits, "seconds": timings, "median_seconds": statistics.median(timings)}))


if __name__ == "__main__":
    main()
