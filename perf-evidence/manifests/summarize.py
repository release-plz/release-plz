import pathlib, re, statistics, json, sys
root = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(__file__).parent
rows = {}
for change, filter_name in [('dependency-scan', 'benchmark_dependency_scan'), ('borrow-updates', 'benchmark_update_manifests')]:
    values = {}
    for candidate in ['baseline', change]:
        for repetition in [1,2,3]:
            text = (root / f'{candidate}-{filter_name}-{repetition}.log').read_text()
            for case, us in re.findall(r'(dependency_scan/[^:]+|update_manifests/[^:]+): ([0-9.]+) us/iteration', text):
                case = case.splitlines()[-1]
                values.setdefault(case, {}).setdefault(candidate, []).append(float(us))
    print(f'\n{change}\n')
    print('| Case | Before µs (runs 1 / 2 / 3) | After µs (runs 1 / 2 / 3) | Mean before → after | Reduction |')
    print('|---|---|---|---|---|')
    for case, observations in values.items():
        before, after = observations['baseline'], observations[change]
        assert len(before)==len(after)==3
        avg_before, avg_after = statistics.mean(before), statistics.mean(after)
        reduction=100*(1-avg_after/avg_before)
        row=dict(before=before, after=after, mean_before_us=avg_before, mean_after_us=avg_after, reduction_percent=reduction)
        rows.setdefault(change, {})[case]=row
        print(f'| {case} | {" / ".join(f"{v:.3f}" for v in before)} | {" / ".join(f"{v:.3f}" for v in after)} | {avg_before:.3f} → {avg_after:.3f} | {reduction:.1f}% |')
(root/'results.json').write_text(json.dumps(rows,indent=2)+'\n')
