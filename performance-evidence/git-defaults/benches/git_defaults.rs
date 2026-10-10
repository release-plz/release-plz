use std::{hint::black_box, time::Duration};
use changelog_git_defaults_evidence::implementation::{self, ChangelogBuilder, Remote};
use chrono::NaiveDate;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use git_cliff_core::{commit::Commit, config::{Config, GitConfig}, contributor::RemoteContributor};

const PR_LINK: &str = "https://github.com/example/repository/pull";

fn configured_git() -> GitConfig {
    implementation::default_git_config(Some(PR_LINK))
}

fn builder(commits: usize, custom: bool) -> ChangelogBuilder<'static> {
    let commits = (0..commits).map(|i| Commit::new(format!("{i:040x}"), format!("fix: handle request {i} (#{i})"))).collect();
    let builder = ChangelogBuilder::new(commits, "1.2.3", "example-package")
        .with_release_date(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap())
        .with_previous_version("1.2.2")
        .with_pr_link(PR_LINK)
        .with_release_link("https://github.com/example/repository/compare/v1.2.2...v1.2.3")
        .with_remote(Remote {
            owner: "example".into(), repo: "repository".into(),
            link: "https://github.com/example/repository".into(),
            contributors: (0..5).map(|i| RemoteContributor {
                username: Some(format!("contributor-{i}")), pr_numbers: vec![i + 1], ..Default::default()
            }).collect(),
        });
    if custom {
        builder.with_config(Config {
            changelog: implementation::default_changelog_config(None),
            git: configured_git(),
            remote: Default::default(),
            bump: Default::default()
        })
    } else {
        builder
    }
}

fn git_defaults(c: &mut Criterion) {
    let custom_git = configured_git();
    let mut group = c.benchmark_group("git_defaults");
    group.sample_size(100).warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(3));
    group.bench_function("configured_phase", |b| b.iter_batched(
        || custom_git.clone(),
        |config| black_box(implementation::benchmark_apply_defaults(black_box(config), Some(PR_LINK))),
        BatchSize::SmallInput,
    ));
    group.bench_function("empty_phase_control", |b| b.iter_batched(
        GitConfig::default,
        |config| black_box(implementation::benchmark_apply_defaults(black_box(config), Some(PR_LINK))),
        BatchSize::SmallInput,
    ));
    for (name, count, custom) in [
        ("build_generate_custom_1", 1, true),
        ("build_generate_custom_20", 20, true),
        ("build_generate_default_control_1", 1, false),
    ] {
        let builder = builder(count, custom);
        let expected = builder.build().generate().unwrap();
        assert!(expected.contains("handle request 0"));
        assert!(expected.contains("[1.2.3]"));
        group.bench_function(name, |b| b.iter_batched(
            || builder.clone(),
            |builder| {
                let result = black_box(builder).build().generate().unwrap();
                assert_eq!(result, expected);
                black_box(result)
            },
            BatchSize::SmallInput,
        ));
    }
    group.finish();
}

criterion_group!(benches, git_defaults);
criterion_main!(benches);
