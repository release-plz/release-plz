use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use next_version::NextVersion as _;
use semver::Version;

fn next_version(c: &mut Criterion) {
    let current = Version::new(1, 0, 0);
    let mut group = c.benchmark_group("next_version");
    for count in [10, 100, 1_000] {
        for (name, message, expected) in [
            ("fixes", "fix: correct behavior", Version::new(1, 0, 1)),
            ("features", "feat: add behavior", Version::new(1, 1, 0)),
            ("breaking", "feat!: change behavior", Version::new(2, 0, 0)),
        ] {
            let commits = vec![message; count];
            assert_eq!(current.next(&commits), expected);
            group.throughput(Throughput::Elements(count.try_into().unwrap()));
            group.bench_with_input(BenchmarkId::new(name, count), &commits, |b, commits| {
                b.iter(|| black_box(&current).next(black_box(commits)));
            });
        }
    }
    group.finish();
}

criterion_group!(benches, next_version);
criterion_main!(benches);
