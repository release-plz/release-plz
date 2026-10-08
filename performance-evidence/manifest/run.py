#!/usr/bin/env python3
"""Build isolated production revisions and compare dependency propagation.

Usage: python3 run.py /path/to/release-plz /path/to/results
Requires cargo/rustc and all three Git commits below to be available locally.
The output directory must not already contain the temporary worktree names.
"""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


REVISIONS = {
    "baseline": "6e42d3c2",
    "indexed": "3cb1bd4a",
    "pathless": "5291c26b",
}
BENCHMARK = "benchmark_dependency_propagation"


def main():
    repository = Path(sys.argv[1]).resolve()
    output = Path(sys.argv[2]).resolve()
    output.mkdir(parents=True, exist_ok=True)
    harness = Path(__file__).resolve().with_name("harness.rs")
    environment = os.environ.copy()
    environment.setdefault("CARGO_TARGET_DIR", str(output / "target"))
    environment.setdefault("CARGO_BUILD_JOBS", "4")
    executables = {}
    hashes = {}
    worktrees = []
    try:
        for name, revision in REVISIONS.items():
            worktree = output / f"worktree-{name}"
            subprocess.run(
                ["git", "-C", str(repository), "worktree", "add", "--detach", str(worktree), revision],
                check=True,
            )
            worktrees.append(worktree)
            source = worktree / "crates/release_plz_core/src/command/update/package_dependencies.rs"
            source.write_text(source.read_text() + f"\ninclude!({json.dumps(str(harness))});\n")
            (worktree / "crates/release_plz_core/src/lib.rs").touch()
            command = [
                "cargo", "test", "--locked", "--release", "-p", "release_plz_core",
                "--lib", "--no-run", "--message-format=json",
            ]
            completed = subprocess.run(command, cwd=worktree, env=environment, text=True, capture_output=True)
            (output / f"build-{name}.jsonl").write_text(completed.stdout)
            (output / f"build-{name}.log").write_text(completed.stderr)
            completed.check_returncode()
            built = None
            for line in completed.stdout.splitlines():
                artifact = json.loads(line)
                if artifact.get("reason") == "compiler-artifact" and artifact.get("executable") and artifact["target"]["name"] == "release_plz_core":
                    built = Path(artifact["executable"])
            assert built is not None, f"Missing test executable for {name}"
            executable = output / f"core-tests-{name}"
            shutil.copy2(built, executable)
            executables[name] = executable
            hashes[name] = hashlib.sha256(executable.read_bytes()).hexdigest()
            with (output / f"tests-{name}.log").open("w") as log:
                subprocess.run(
                    [str(executable), "package_dependencies::tests", "--nocapture", "--test-threads=1"],
                    cwd=worktree, env=environment, stdout=log, stderr=subprocess.STDOUT, check=True,
                )

        assert len(set(hashes.values())) == len(hashes), "Expected distinct revision executables"
        (output / "binary-hashes.json").write_text(json.dumps(hashes, indent=2) + "\n")
        environment["CARGO_NET_OFFLINE"] = "true"

        # No builds or unrelated tests run during measurement. Reverse the first
        # order on run 2 and vary run 3 to reduce systematic ordering effects.
        orders = [
            ["baseline", "indexed", "pathless"],
            ["pathless", "indexed", "baseline"],
            ["baseline", "pathless", "indexed"],
        ]
        for run, order in enumerate(orders, start=1):
            for name in order:
                print(f"Run {run}: {name}", flush=True)
                with (output / f"{name}-{run}.log").open("w") as log:
                    subprocess.run(
                        [str(executables[name]), BENCHMARK, "--ignored", "--nocapture", "--test-threads=1"],
                        cwd=repository, env=environment, stdout=log, stderr=subprocess.STDOUT, check=True,
                    )
    finally:
        for worktree in worktrees:
            subprocess.run(["git", "-C", str(repository), "worktree", "remove", "--force", str(worktree)], check=True)


if __name__ == "__main__":
    main()
