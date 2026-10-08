import json, statistics
from pathlib import Path
root = Path('/workspace/perf-results/history')
scenarios = ('history_100', 'workspace_10_one_change')
for variant, description, tests in (
    ('cached-lock', 'History package equality already caches Cargo’s file list and restores `Cargo.lock` for the current snapshot. Reusing that list while checking changed files now avoids a second `git checkout Cargo.lock`; uncached listings still restore the lockfile after running Cargo.', '21 existing update integration tests passed'),
    ('readme', 'Compare packaged contents before querying README metadata. A historical snapshot whose packaged files already differ can skip `cargo metadata`; equal or inconclusive file comparisons still check the README, including README files outside the package. Lockfile-restoration failures remain fatal, and a changed README can still establish inequality when package listing fails.', '22 update integration tests passed, including a new regression covering unchanged and changed external READMEs'),
):
    rows = []
    reductions = {}
    for scenario in scenarios:
        values = {}
        for name in ('before', variant):
            values[name] = [json.loads((root/'estimates'/f'{name}-{run}'/f'{scenario}.json').read_text())['mean']['point_estimate']/1e6 for run in (1,2,3)]
        before,after=(statistics.median(values[name]) for name in ('before', variant))
        reductions[scenario] = (1-after/before)*100
        fmt=lambda vs:', '.join(f'{v:.2f}' for v in vs)
        rows.append(f'| `{scenario}` | {fmt(values["before"])} | {fmt(values[variant])} | {before:.2f} → {after:.2f} | {reductions[scenario]:.2f}% |')
    body = description + '\n\n'
    body += f'Observed `update/history_100` elapsed time fell **{reductions["history_100"]:.2f}%** (median of three independent Criterion mean estimates).\n\n'
    body += '| #3146 scenario | Before: runs 1, 2, 3 (ms) | After: runs 1, 2, 3 (ms) | Median before → after (ms) | Time reduction |\n| --- | --- | --- | --- | --- |\n' + '\n'.join(rows)
    body += '\n\nThe workspace control varied more across runs than the observed median difference; no workspace speedup is claimed.\n\nMeasured against main `72465264` on Linux x86_64 with Rust 1.99.0 and optimized default-feature builds. The [PR #3146](https://github.com/release-plz/release-plz/pull/3146) update benchmarks used their unchanged settings (10 samples, 1 s warmup, 5 s measurement target; the history scenario automatically takes longer to collect all samples). Each variant ran three times, interleaved under an exclusive benchmark lock. Fixture setup/reset and version/changelog checks are outside the timer; every measured command runs offline and validates its result. Benchmark infrastructure is excluded from this PR.\n\n'
    body += 'Validation: `cargo test --locked --release -p release-plz --test all update::` — ' + tests + '.\n\n'
    body += f'Raw logs, estimates, commands, and patches: [performance evidence](https://github.com/release-plz/release-plz/tree/codex/performance-evidence-20261008/history).\n'
    if variant == 'cached-lock':
        body = 'Investigated but not proposed as a PR. The three history rounds all favored this change, but the final individual Criterion comparison reported p = 0.09 (no detected change), and the first comparison was within the practical noise threshold. Retained as local code and evidence only.\n\n' + body
        (root/'cached-lock-investigation.md').write_text(body)
    else:
        (root/f'{variant}-pr-body.md').write_text(body)
