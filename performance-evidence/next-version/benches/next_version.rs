use std::{hint::black_box, time::Duration};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use next_version::NextVersion as _;
use semver::Version;

// The unchanged next_version suite from #3143 (the base of #3146).
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

// The same histories with the documented 0.0.x version policy.
fn zero_zero(c: &mut Criterion) {
    let current = Version::new(0, 0, 1);
    let mut group = c.benchmark_group("zero_zero");
    for count in [10, 100, 1_000] {
        for (name, message) in [
            ("fixes", "fix: correct behavior"),
            ("features", "feat: add behavior"),
            ("breaking", "feat!: change behavior"),
        ] {
            let commits = vec![message; count];
            assert_eq!(current.next(&commits), Version::new(0, 0, 2));
            group.throughput(Throughput::Elements(count.try_into().unwrap()));
            group.bench_with_input(BenchmarkId::new(name, count), &commits, |b, commits| {
                b.iter(|| black_box(&current).next(black_box(commits)));
            });
        }
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(40)
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(1))
        .output_directory(std::path::Path::new("/workspace/perf-evidence/next-version/results"));
    targets = next_version, zero_zero
}
criterion_main!(benches);
