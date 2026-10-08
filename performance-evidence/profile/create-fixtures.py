from pathlib import Path
import subprocess
import shutil
root=Path(__file__).resolve().parent/'fixtures'
for count in (100, 1000):
    repo=root/f'tracked-{count}'
    if repo.exists():
        shutil.rmtree(repo)
    repo.mkdir(parents=True, exist_ok=True)
    (repo/'.gitignore').write_text('target/\n')
    for i in range(count):
        file=repo/f'crates/crate-{i//100}/src/modules/group-{i//10}/module-{i}.rs'
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(f'// Module {i}\n' + 'pub fn example() -> bool { true }\n'*8)
    (repo/'target').mkdir(exist_ok=True)
    (repo/'target/ignored.txt').write_text('ignored build output')
    def git(*args):
        subprocess.run(['git','-C',str(repo),*args],check=True,capture_output=True)
    git('init','-b','main')
    git('add','.')
    git('-c','user.name=Benchmark','-c','user.email=benchmark@example.invalid','commit','-m','fixture')
    git('gc','--prune=now')
