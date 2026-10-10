use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use next_version::{NextVersion as _, VersionUpdater};
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

fn prerelease(c: &mut Criterion) {
    let current = Version::parse("1.0.0-alpha.1").unwrap();
    let expected = Version::parse("1.0.0-alpha.2").unwrap();
    let long = format!(
        "fix: correct behavior\n\n{}",
        "Explain the behavior and implementation.\n".repeat(100)
    );
    let mut group = c.benchmark_group("prerelease");
    for (name, message) in [
        ("short", "fix: correct behavior"),
        ("long_body", long.as_str()),
    ] {
        let commits = vec![message; 100];
        assert_eq!(current.next(&commits), expected);
        group.bench_with_input(name, &commits, |b, commits| {
            b.iter(|| black_box(&current).next(black_box(commits)));
        });
    }
    group.finish();
}

fn zero_major(c: &mut Criterion) {
    let current = Version::new(0, 2, 3);
    let expected = Version::new(0, 3, 0);
    let mut group = c.benchmark_group("zero_major");
    for count in [10, 100, 1_000] {
        for (name, features_minor, message) in [
            ("breaking", false, "feat!: change behavior"),
            ("features", true, "feat: add behavior"),
        ] {
            let commits = vec![message; count];
            assert_eq!(
                VersionUpdater::new()
                    .with_features_always_increment_minor(features_minor)
                    .increment(&current, &commits),
                expected
            );
            group.bench_with_input(BenchmarkId::new(name, count), &commits, |b, commits| {
                b.iter(|| {
                    black_box(
                        VersionUpdater::new().with_features_always_increment_minor(features_minor),
                    )
                    .increment(black_box(&current), black_box(commits))
                });
            });
        }
    }
    group.finish();
}

criterion_group!(benches, next_version, prerelease, zero_major);
criterion_main!(benches);
