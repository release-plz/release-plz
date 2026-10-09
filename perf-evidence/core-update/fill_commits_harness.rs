
#[cfg(test)]
mod perf_bench_fill_commits {
    use super::*;
    use std::{hint::black_box, time::Instant};

    #[tokio::test]
    #[ignore]
    async fn benchmark_fill_commits() {
        let directory = crate::fs_utils::Utf8TempDir::new().unwrap();
        let root = directory.path();
        crate::test_utils::write_package(root, "benchmark-package", "1.0.0", "");
        let repository = Repo::init(root);
        let manifest = root.join("Cargo.toml");
        let metadata = cargo_utils::get_manifest_metadata(&manifest).unwrap();
        for configured in [false, true] {
            let mut req = UpdateRequest::new(metadata.clone()).unwrap();
            if configured {
                let mut config = Config {
                    changelog: crate::changelog::default_changelog_config(None),
                    git: crate::changelog::default_git_config(None),
                    remote: Default::default(),
                    bump: Default::default(),
                };
                config.changelog.body = "{% for commit in commits %}- {{ commit.message }}{% endfor %}".to_owned();
                req = req.with_changelog_req(ChangelogRequest {
                    changelog_config: Some(config),
                    ..Default::default()
                });
            }
            let project = Project::new(&manifest, None, &HashSet::new(), req.cargo_metadata(), &req).unwrap();
            let updater = Updater { project: &project, req: &req };
            let packages = project.workspace_packages();
            for (commits, iterations) in [(100, 10_000), (1000, 2_000)] {
                let mut diff = Diff::new(true);
                diff.commits = (0..commits).map(|index| Commit::new(
                    format!("{index:040x}"),
                    format!("fix: correct parser handling for input case {index}"),
                )).collect();
                let seed = vec![(packages[0], diff)];
                // Duplicate IDs exercise cache hits while messages remain distinct.
                let mut input = seed.clone();
                for commit in &mut input[0].1.commits {
                    commit.id = "0123456789abcdef0123456789abcdef01234567".to_owned();
                }
                let expected = input[0].1.commits.clone();
                let output = updater.fill_commits(BENCH_INPUT, &repository).await.unwrap();
                assert_eq!(output[0].1.commits, expected);
                let mut total = std::time::Duration::ZERO;
                for iteration in 0..iterations + 100 {
                    // Fixture cloning is outside the measured region in both versions.
                    let input = black_box(seed.clone());
                    let start = Instant::now();
                    let output = updater.fill_commits(BENCH_INPUT, &repository).await.unwrap();
                    let elapsed = start.elapsed();
                    assert_eq!(output[0].1.commits, seed[0].1.commits);
                    black_box(&output);
                    if iteration >= 100 { total += elapsed; }
                }
                let nanos = total.as_nanos() as f64 / iterations as f64;
                println!("FILL_COMMITS configured={configured} commits={commits} iterations={iterations} ns_per_call={nanos:.3}");
            }
        }
    }
}
