use std::hint::black_box;

use chrono::NaiveDate;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use git_cliff_core::commit::Commit;
use release_plz_core::{ChangelogBuilder, default_git_config};

fn changelog_render(c: &mut Criterion) {
    c.bench_function("default_git_config", |b| {
        b.iter(|| black_box(default_git_config(black_box(None))));
    });
    let mut group = c.benchmark_group("changelog_generate");
    for count in [10, 100] {
        let commits = (0..count)
            .map(|i| Commit::new(format!("{i:040x}"), "fix: correct behavior".to_string()))
            .collect();
        let builder = ChangelogBuilder::new(commits, "1.0.1", "benchmark")
            .with_release_date(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        let output = builder.build().generate().unwrap();
        assert_eq!(output.matches("- correct behavior").count(), count);
        group.bench_with_input(BenchmarkId::from_parameter(count), &builder, |b, builder| {
            b.iter(|| black_box(builder).build().generate().unwrap());
        });
    }
    group.finish();
}

criterion_group!(benches, changelog_render);
criterion_main!(benches);
