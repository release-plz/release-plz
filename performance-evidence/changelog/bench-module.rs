// Local performance evidence only; excluded from performance PRs.
#[cfg(test)]
mod perf_changelog_evidence {
    use super::*;
    use chrono::NaiveDate;
    use std::{
        hint::black_box,
        time::{Duration, Instant},
    };

    fn fingerprint(value: impl std::hash::Hash) -> u64 {
        use std::hash::Hasher;
        let mut hasher = std::hash::DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    fn commits(count: usize, metadata: bool) -> Vec<Commit> {
        (0..count)
            .map(|index| {
                let mut commit =
                    Commit::new(format!("{index:040x}"), format!("fix: change {index}"));
                if metadata {
                    commit.author.name = Some(format!("Contributor {}", index % 20));
                    commit.author.email = Some(format!("contributor{}@example.com", index % 20));
                    commit.committer = commit.author.clone();
                    commit.remote.username = Some(format!("contributor{}", index % 20));
                    commit.remote.pr_number = Some(index as i64);
                }
                commit
            })
            .collect()
    }

    fn measure<T, O>(name: &str, mut prepare: impl FnMut() -> T, mut run: impl FnMut(T) -> O) {
        for _ in 0..5 {
            black_box(run(prepare()));
        }
        let mut samples = Vec::new();
        for _ in 0..25 {
            let mut elapsed = Duration::ZERO;
            let mut iterations = 0;
            while elapsed < Duration::from_millis(100) {
                let input = prepare();
                let start = Instant::now();
                black_box(run(input));
                elapsed += start.elapsed();
                iterations += 1;
            }
            samples.push(elapsed.as_secs_f64() * 1e6 / f64::from(iterations));
        }
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        println!("BENCH {name} mean_us={mean:.3} samples_us={samples:?}");
    }

    #[test]
    #[ignore]
    fn perf_changelog_generate() {
        for count in [100, 1000] {
            for metadata in [false, true] {
                let commits = commits(count, metadata);
                let builder = ChangelogBuilder::new(
                    commits.iter().map(Commit::to_cliff_commit).collect(),
                    "1.0.1",
                    "package-0",
                )
                .with_release_date(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
                let expected = builder.build().generate().unwrap();
                assert!(expected.contains(&format!("- change {}", count - 1)));
                println!(
                    "OUTPUT generate/{count}/{metadata}: {} bytes, hash {:x}",
                    expected.len(),
                    fingerprint(&expected)
                );
                measure(
                    &format!("generate/{count}/metadata_{metadata}"),
                    || builder.build(),
                    |changelog| changelog.generate().unwrap(),
                );
            }
        }
    }

    #[test]
    #[ignore]
    fn perf_changelog_update() {
        let package: Package = fake_package::FakePackage::new("package-0").into();
        let next_version = Version::new(1, 0, 1);
        let old = OldChangelog::new(None, false);
        for count in [100, 1000] {
            for metadata in [false, true] {
                let commits = commits(count, metadata);
                let config = Config {
                    changelog: ChangelogConfig {
                        header: Some("# Changelog\n".to_string()),
                        body: "## [{{ version }}] - {{ timestamp | date(format=\"%Y-%m-%d\") }}\n{% for commit in commits %}- {{ commit.message }}\n{% endfor %}".to_string(),
                        ..Default::default()
                    },
                    git: crate::changelog::default_git_config(None),
                    remote: Default::default(),
                    bump: Default::default(),
                };
                let request = ChangelogRequest {
                    release_date: NaiveDate::from_ymd_opt(2025, 1, 1),
                    changelog_config: Some(config),
                };
                let generate = |request| {
                    get_changelog(
                        &commits,
                        &next_version,
                        Some(request),
                        &old,
                        None,
                        &package,
                        &package.version,
                    )
                    .unwrap()
                };
                let expected = generate(request.clone());
                assert!(expected.0.contains(&format!("- change {}", count - 1)));
                assert!(expected.1.contains(&format!("- change {}", count - 1)));
                println!(
                    "OUTPUT update/{count}/{metadata}: {} + {} bytes, hash {:x}",
                    expected.0.len(),
                    expected.1.len(),
                    fingerprint(&expected)
                );
                measure(
                    &format!("update/{count}/metadata_{metadata}"),
                    || request.clone(),
                    generate,
                );
            }
        }
    }
}
