from pathlib import Path
import resource,subprocess,sys
revision,out=sys.argv[1:]
with open(out+'.jsonl','w') as stream:
    subprocess.run(['/workspace/perf-results/conventional/conventional-bench',revision],stdout=stream,check=True)
Path(out+'.resources').write_text(f'peak_rss_kb={resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss}\n')
