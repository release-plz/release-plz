#!/usr/bin/env python3
"""Local extension of PR #3146's update.rs CLI fixture; never shipped in a PR.

Runs the same offline `release-plz update` workload with explicit before/after
binaries. `--tagged` adds one release tag per package to exercise an empty Git
history range. Each of three repetitions has a warmup and N measured updates;
fixture creation, Git reset, and output validation are outside the timer.
"""
import argparse
import collections
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time
import tomllib


def command(args, cwd, env=None):
    result = subprocess.run([str(a) for a in args], cwd=cwd, env=env,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise RuntimeError(f'{args!r} failed ({result.returncode})\n'
                           f'{result.stdout.decode()}\n{result.stderr.decode()}')
    return result.stdout.decode()


def fixture(root, count, tagged):
    for name in ['current', 'released']:
        directory = root / name
        directory.mkdir()
        (directory / 'Cargo.toml').write_text(
            '[workspace]\nmembers = ["packages/*"]\nresolver = "3"\n')
        for index in range(count):
            package = directory / 'packages' / f'package-{index}'
            (package / 'src').mkdir(parents=True)
            (package / 'Cargo.toml').write_text(
                f'[package]\nname = "package-{index}"\nversion = "1.0.0"\nedition = "2024"\n')
            (package / 'src/lib.rs').write_text('// Initial release\n')
        command(['cargo', 'generate-lockfile', '--offline'], directory)
    current = root / 'current'
    (current / '.gitattributes').write_text('* -text\n')
    (current / 'README.md').write_text('test\n')
    (current / 'release-plz.toml').write_text('[workspace]\nsemver_check = false\n')
    (current / 'cliff.toml').write_text('''[changelog]
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
    command(['git', 'init', '-b', 'main'], current)
    command(['git', 'config', 'user.name', 'Benchmark'], current)
    command(['git', 'config', 'user.email', 'bench@example.com'], current)
    command(['git', 'config', 'commit.gpgsign', 'false'], current)
    command(['git', 'config', 'tag.gpgsign', 'false'], current)
    command(['git', 'add', '.'], current)
    command(['git', 'commit', '-m', 'add README'], current)
    if tagged:
        for index in range(count):
            command(['git', 'tag', f'package-{index}-v1.0.0'], current)
    return current


def measure(binary, current, count, trace=None):
    root = current.parent
    command(['git', 'reset', '--hard', 'HEAD'], current)
    command(['git', 'clean', '-fd'], current)
    env = os.environ.copy()
    env.update(CARGO_NET_OFFLINE='true', CARGO_TARGET_DIR=str(root / 'target'),
               RELEASE_PLZ_NO_ANSI='1', RELEASE_PLZ_LOG='error')
    env.pop('GIT_TOKEN', None)
    env.pop('GIT_TRACE2_EVENT', None)
    if trace:
        env['GIT_TRACE2_EVENT'] = str(trace)
    argv = [binary, 'update', '--release-date', '2025-01-01', '--changelog-config',
            'cliff.toml', '--registry-manifest-path', root / 'released/Cargo.toml']
    start = time.perf_counter()
    command(argv, current, env)
    elapsed = time.perf_counter() - start
    for index in range(count):
        package = current / 'packages' / f'package-{index}'
        assert tomllib.loads((package / 'Cargo.toml').read_text())['package']['version'] == '1.0.0'
        assert not (package / 'CHANGELOG.md').exists()
    assert not command(['git', 'status', '--porcelain'], current).strip()
    return elapsed


def trace_counts(path):
    counts = collections.Counter()
    starts = []
    for line in path.read_text().splitlines():
        event = json.loads(line)
        if event.get('event') == 'start':
            args = event['argv']
            if len(args) > 2 and args[1] == '-C':
                args = [args[0]] + args[3:]
            starts.append(args)
            counts[args[1] if len(args) > 1 else '(none)'] += 1
            if args[1:] == ['log', '-1', '--pretty=format:%H']:
                counts['head hash'] += 1
    return {'total_git_commands': len(starts), 'command_counts': dict(counts)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', type=Path, required=True)
    parser.add_argument('--after', type=Path, required=True)
    parser.add_argument('--packages', type=int, default=10)
    parser.add_argument('--tagged', action='store_true')
    parser.add_argument('--iterations', type=int, default=5)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--expect-checkout-reduction', type=int)
    parser.add_argument('--expect-head-read-reduction', type=int)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    binaries = {'before': args.before.resolve(), 'after': args.after.resolve()}
    results = {'packages': args.packages, 'tagged': args.tagged,
               'iterations': args.iterations, 'binaries': {}, 'runs': [], 'profile': {}}
    for label, path in binaries.items():
        results['binaries'][label] = {
            'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
            'version': command([path, '--version'], path.parent).strip()}
    assert results['binaries']['before']['sha256'] != results['binaries']['after']['sha256']
    with tempfile.TemporaryDirectory(prefix='release-plz-git-bench-') as directory:
        current = fixture(Path(directory), args.packages, args.tagged)
        for label, binary in binaries.items():
            trace = args.output.resolve() / f'{label}-trace.jsonl'
            trace.unlink(missing_ok=True)
            measure(binary, current, args.packages, trace)
            results['profile'][label] = trace_counts(trace)
        for argument, key in [(args.expect_checkout_reduction, 'checkout'),
                              (args.expect_head_read_reduction, 'head hash')]:
            if argument is not None:
                before = results['profile']['before']['command_counts'].get(key, 0)
                after = results['profile']['after']['command_counts'].get(key, 0)
                assert before - after == argument, (key, before, after, argument)
        for repetition in range(3):
            result = {'repetition': repetition + 1}
            # Alternate ordering to avoid consistently favoring either binary.
            order = ['before', 'after'] if repetition % 2 == 0 else ['after', 'before']
            for label in order:
                measure(binaries[label], current, args.packages)
                times = [measure(binaries[label], current, args.packages)
                         for _ in range(args.iterations)]
                result[label] = {'seconds': times, 'mean': statistics.mean(times),
                                 'median': statistics.median(times)}
            result['improvement_percent'] = 100 * (1 - result['after']['mean'] / result['before']['mean'])
            results['runs'].append(result)
            (args.output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
            print(json.dumps(result), flush=True)
    results['before_mean'] = statistics.mean(run['before']['mean'] for run in results['runs'])
    results['after_mean'] = statistics.mean(run['after']['mean'] for run in results['runs'])
    results['improvement_percent'] = 100 * (1 - results['after_mean'] / results['before_mean'])
    (args.output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    print(json.dumps({k: v for k, v in results.items() if k != 'runs'}, indent=2))


if __name__ == '__main__':
    main()
