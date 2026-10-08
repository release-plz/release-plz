use chrono::NaiveDate;
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use git_cliff_core::{commit::Commit, config::Config};
use release_plz_core::{ChangelogBuilder, default_changelog_config, default_git_config};
use std::{hint::black_box, time::Duration};

fn builder(c: &mut Criterion) {
    let mut group = c.benchmark_group("changelog_builder");
    group
        .sample_size(50)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    for count in [1, 10, 1000] {
        let builder = ChangelogBuilder::new(
            (0..count)
                .map(|i| Commit::new(format!("{i:040x}"), format!("fix: repair issue {i}")))
                .collect(),
            "1.0.1",
            "example",
        )
        .with_release_date(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap())
        .with_config(Config {
            changelog: default_changelog_config(None),
            git: default_git_config(None),
            remote: Default::default(),
            bump: Default::default(),
        });
        let output = builder.build().generate().unwrap();
        assert_eq!(output.matches("- repair issue ").count(), count);
        assert!(output.contains("## [1.0.1] - 2025-01-01"));
        group.bench_with_input(BenchmarkId::new("configured", count), &count, |b, _| {
            // Each package starts with a fresh builder/configuration. Cloning the
            // fixture is setup, outside the timed build.
            b.iter_batched_ref(
                || builder.clone(),
                |builder| black_box(builder.build()),
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}
criterion_group!(benches, builder);
criterion_main!(benches);
