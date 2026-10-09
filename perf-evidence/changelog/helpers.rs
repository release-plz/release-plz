pub fn perf_config(with_pr: bool) -> Config {
    let mut changelog = Changelog {
        release: Release::default(),
        config: None,
        release_link: None,
        package: "example".into(),
        remote: None,
        pr_link: with_pr.then(|| "https://github.com/example/project/pull".into()),
    };
    changelog.changelog_config(None)
}

pub fn remote_fixture(count: usize) -> Remote {
    Remote {
        owner: "example".into(),
        repo: "project".into(),
        link: "https://github.com/example/project".into(),
        contributors: (0..count).map(|i| RemoteContributor {
            username: Some(format!("contributor-{i}")),
            pr_number: Some(i as i64 + 1),
            pr_numbers: vec![i as i64 + 1, i as i64 + count as i64 + 1],
            is_first_time: i % 2 == 0,
            ..Default::default()
        }).collect(),
    }
}

pub fn context_fixture(count: usize) -> impl FnMut() {
    let remote = remote_fixture(count);
    let config = Config {
        changelog: ChangelogConfig { header: None, body: "{{ version }}".into(), ..Default::default() },
        git: default_git_config(None),
        remote: RemoteConfig::default(),
        bump: Bump::default(),
    };
    let mut changelog = GitCliffChangelog::new(vec![], config, None).unwrap();
    move || add_remote_context(std::hint::black_box(&mut changelog), std::hint::black_box(Some(&remote))).unwrap()
}

pub fn generate_fixture(contributors: usize, prepend: bool, with_pr: bool) -> impl FnMut() -> String {
    let mut builder = ChangelogBuilder::new(
        vec![Commit::new("0123456789abcdef0123456789abcdef01234567".into(), "fix: improve automation (#123)".into())],
        "1.2.3",
        "example",
    ).with_release_date(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
    if with_pr { builder = builder.with_pr_link("https://github.com/example/project/pull"); }
    if contributors > 0 { builder = builder.with_remote(remote_fixture(contributors)); }
    let old = format!("{CHANGELOG_HEADER}\n## [1.2.2] - 2024-01-01\n\n- Prior release\n");
    move || {
        let changelog = builder.build();
        if prepend { changelog.prepend(old.as_str()).unwrap() } else { changelog.generate().unwrap() }
    }
}

pub fn remote_output(count: usize) -> String {
    let config = Config {
        changelog: ChangelogConfig {
            header: None,
            body: "{{ remote.owner }}/{{ remote.repo }} {{ remote.link }} {% for c in remote.contributors %}{{ c.username }} {{ c.pr_number }} {{ c.is_first_time }} {% for n in c.pr_numbers %}{{ n }} {% endfor %}{% endfor %}".into(),
            ..Default::default()
        },
        git: default_git_config(None),
        remote: RemoteConfig::default(),
        bump: Bump::default(),
    };
    ChangelogBuilder::new(vec![Commit::new("0123456789abcdef0123456789abcdef01234567".into(), "fix: change".into())], "1.2.3", "example")
        .with_config(config)
        .with_remote(remote_fixture(count))
        .with_release_date(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap())
        .build().generate().unwrap()
}
