use std::hint::black_box;

use chrono::NaiveDate;
use criterion::{Criterion, criterion_group, criterion_main};
use git_cliff_core::{
    commit::Commit,
    config::{Bump, Config, RemoteConfig},
};
use release_plz_core::{ChangelogBuilder, default_changelog_config, default_git_config};

fn changelog(c: &mut Criterion) {
    let commits = (0..10)
        .map(|index| Commit::new(format!("{index:040x}"), format!("fix: change {index}")))
        .collect();
    let builder = ChangelogBuilder::new(commits, "1.0.1", "example")
        .with_release_date(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
    let config = Config {
        changelog: default_changelog_config(None),
        git: default_git_config(None),
        remote: RemoteConfig::default(),
        bump: Bump::default(),
    };
    let configured = builder.clone().with_config(config);
    let expected = builder.build().generate().unwrap();
    assert_eq!(configured.build().generate().unwrap(), expected);
    assert!(expected.contains("1.0.1"));
    for index in 0..10 {
        assert!(expected.contains(&format!("change {index}")));
    }

    let mut group = c.benchmark_group("changelog");
    for (name, builder) in [("default_config", builder), ("explicit_config", configured)] {
        group.bench_function(name, |b| {
            b.iter(|| black_box(builder.build().generate().unwrap()));
        });
    }
    group.finish();
}

criterion_group!(benches, changelog);
criterion_main!(benches);
