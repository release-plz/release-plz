use std::{hint::black_box, time::Duration};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use semver::Version;

// Same 9 input cases and assertions as PR #3146. The revision dimension allows
// direct, alternating runs against the exact main source and the candidate.
fn next_version(c: &mut Criterion) {
    let current = Version::new(1, 0, 0);
    let mut group = c.benchmark_group("next_version");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(50);
    for revision in ["before", "after"] {
        for count in [10, 100, 1_000] {
            for (name, message, expected) in [
                ("fixes", "fix: correct behavior", Version::new(1, 0, 1)),
                ("features", "feat: add behavior", Version::new(1, 1, 0)),
                ("breaking", "feat!: change behavior", Version::new(2, 0, 0)),
            ] {
                let commits = vec![message; count];
                group.throughput(Throughput::Elements(count.try_into().unwrap()));
                let id = BenchmarkId::new(format!("{revision}/{name}"), count);
                if revision == "before" {
                    use next_version_baseline::NextVersion as _;
                    assert_eq!(current.next(&commits), expected);
                    group.bench_with_input(id, &commits, |b, commits| {
                        b.iter(|| black_box(&current).next(black_box(commits)));
                    });
                } else {
                    use next_version::NextVersion as _;
                    assert_eq!(current.next(&commits), expected);
                    group.bench_with_input(id, &commits, |b, commits| {
                        b.iter(|| black_box(&current).next(black_box(commits)));
                    });
                }
            }
        }
    }
    group.finish();
}

criterion_group!(benches, next_version);
criterion_main!(benches);
