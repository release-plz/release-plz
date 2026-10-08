#![allow(dead_code)]
use std::{hint::black_box, time::Duration};
use criterion::{Criterion, criterion_group, criterion_main};
use git_cliff_core::commit::Commit;
mod before;
mod after;
mod changelog_parser;
pub const NO_COMMIT_ID: &str = "0000000";

fn changelog(c: &mut Criterion) {
    let mut group = c.benchmark_group("changelog_pr_regex");
    group.sample_size(50).warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(3));
    for (name, config) in [("before", before::default_git_config as fn(Option<&str>) -> git_cliff_core::config::GitConfig), ("after", after::default_git_config)] {
        group.bench_function(format!("default_git_config/{name}"), |b| {
            b.iter(|| config(black_box(Some("https://github.com/example/repo/pull"))));
        });
    }
    let commits = vec![Commit::new("abc123".to_owned(), "fix: correct parsing (#123)".to_owned())];
    let before = before::ChangelogBuilder::new(commits.clone(), "1.0.1", "example")
        .with_pr_link("https://github.com/example/repo/pull")
        .with_release_date(chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
    let after = after::ChangelogBuilder::new(commits, "1.0.1", "example")
        .with_pr_link("https://github.com/example/repo/pull")
        .with_release_date(chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
    let expected = before.build().generate().unwrap();
    assert!(expected.contains("[#123](https://github.com/example/repo/pull/123)"));
    assert_eq!(expected, after.build().generate().unwrap());
    group.bench_function("build_and_generate/before", |b| {
        b.iter(|| black_box(&before).build().generate().unwrap());
    });
    group.bench_function("build_and_generate/after", |b| {
        b.iter(|| black_box(&after).build().generate().unwrap());
    });
    group.finish();
}
criterion_group!(benches, changelog);
criterion_main!(benches);
