from pathlib import Path
import json, statistics
root=Path(__file__).resolve().parent
rows=[]
for kind in ['breaking','fixes','features']:
    for count in [10,100,1000]:
        row={'case':f'{kind}/{count}'}
        for revision in ['before','after']:
            row[revision]=[json.loads((root/'criterion'/'next_version'/f'{revision}_{kind}'/str(count)/f'run-{run}-{revision}'/'estimates.json').read_text())['mean']['point_estimate'] for run in [1,2,3]]
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
