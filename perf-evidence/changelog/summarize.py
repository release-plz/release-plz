import hashlib, json, pathlib, re, statistics
root=pathlib.Path(__file__).resolve().parent
cases={}
for run in range(1,4):
    data=(root/f'run-{run}.log').read_text()
    assert data.startswith('VALIDATION all remote rendering and default config/render/prepend checks passed')
    rows=re.findall(r'^RESULT (\w+) before_ns=([\d.]+) after_ns=([\d.]+) reduction_percent=([\d.-]+)$',data,re.M)
    assert len(rows)==9, (run,len(rows))
    for name,a,b,p in rows:
        cases.setdefault(name,[]).append({'run':run,'before_ns':float(a),'after_ns':float(b),'reduction_percent':float(p)})
result={'base_commit':'267b5775','remote_commit':'11cff443','default_commit':'a005b433','method':'Three fresh processes. Each process alternates 21 before/after blocks of at least 100ms after 100ms warmup per variant; operations in batches of 10. Run values are block medians. Identical dependency rlibs and opt-level=3 for both exact-source snapshots.','binary_sha256':hashlib.sha256((root/'benchmark').read_bytes()).hexdigest(),'cases':{}}
for case,runs in cases.items():
    a=statistics.median(r['before_ns'] for r in runs)
    b=statistics.median(r['after_ns'] for r in runs)
    result['cases'][case]={'runs':runs,'median_before_ns':a,'median_after_ns':b,'median_reduction_percent':100*(1-b/a)}
    print(f'{case:24s}: '+ ' | '.join(f"{r['before_ns']/1000:.3f} → {r['after_ns']/1000:.3f} µs ({r['reduction_percent']:.2f}%)" for r in runs))
(root/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
