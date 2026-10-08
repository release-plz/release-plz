from pathlib import Path
from statistics import median
import json
root=Path(__file__).resolve().parent
rows=[]
for benchmark, sizes, unit in [('copy',[100,1000],'us'),('order',[1,10,100,500],'ns')]:
    for size in sizes:
        results={variant:[float((root/f'results/{benchmark}-{size}-{variant}-{run}.csv').read_text().strip().split(',')[-1]) for run in [1,2,3]] for variant in ['before','after']}
        before,after=median(results['before']),median(results['after'])
        row={'benchmark':benchmark,'size':size,'unit':unit,**results,'median_before':before,'median_after':after,'reduction_percent':100*(before-after)/before,'speedup':before/after}
        rows.append(row)
        print(f"{benchmark} {size}: {results['before']} -> {results['after']} {unit}; median reduction {row['reduction_percent']:.2f}%")
(root/'results/summary.json').write_text(json.dumps(rows,indent=2)+'\n')
