from pathlib import Path
import json,statistics
root=Path(__file__).resolve().parent
rows=[]
for name in ['conventional','body_1k','nonconventional']:
    row={'case':name}
    for revision in ['before','after']:
        row[revision]=[next(v['ns_per_call'] for v in map(json.loads,(root/f'run-{run}-{revision}.jsonl').read_text().splitlines()) if v['case']==name) for run in [1,2,3]]
    row['before_median_ns']=statistics.median(row['before'])
    row['after_median_ns']=statistics.median(row['after'])
    row['median_reduction_pct']=100*(1-row['after_median_ns']/row['before_median_ns'])
    row['before_mean_ns']=statistics.mean(row['before'])
    row['after_mean_ns']=statistics.mean(row['after'])
    row['reduction_pct']=100*(1-row['after_mean_ns']/row['before_mean_ns'])
    rows.append(row)
(root/'summary.json').write_text(json.dumps(rows,indent=2)+'\n')
lines=['| Case | Before (ns/call), runs 1 / 2 / 3 | After (ns/call), runs 1 / 2 / 3 | Reduction of three-run mean |','|---|---:|---:|---:|']
for row in rows:
    before=' / '.join(f'{n:.2f}' for n in row['before'])
    after=' / '.join(f'{n:.2f}' for n in row['after'])
    lines.append(f"| {row['case']} | {before} | {after} | {row['reduction_pct']:.2f}% |")
(root/'summary.md').write_text('\n'.join(lines)+'\n')
print('\n'.join(lines))
