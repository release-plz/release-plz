    fn client_request(body: Option<&str>) -> UpdateRequest {
        let mut req = request().with_git_client(crate::GitForge::Github(
            crate::GitHub::new("owner".into(), "repo".into(), secrecy::SecretString::from("benchmark-token"))
                .with_base_url("http://127.0.0.1:9".parse().unwrap()),
        ));
        if let Some(body) = body {
            let mut config: git_cliff_core::config::Config = serde_json::from_value(serde_json::json!({})).unwrap();
            config.changelog.body = body.to_owned();
            req = req.with_changelog_req(ChangelogRequest {
                changelog_config: Some(config),
                ..Default::default()
            });
        }
        req
    }

    #[test]
    #[ignore]
    fn lazy_client() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        for commits in [0, 100] {
            for template in [false, true] {
                let req = client_request(template.then_some("{% for commit in commits %}{{ commit.message }}{% endfor %}"));
                let project = project(&req);
                let updater = Updater { project: &project, req: &req };
                let repo = Repo::new(project.root()).unwrap();
                let package: Package = fake_package::FakePackage::new("package").into();
                let mut diff = Diff::new(true);
                diff.commits = (0..commits).map(|i| Commit::new(format!("{i:040x}"), format!("fix: update change {i}"))).collect();
                let mut diffs = vec![(&package, diff)];
                measure(&format!("client_{commits}_template_{template}"), || {
                    diffs = runtime.block_on(updater.fill_commits(std::mem::take(&mut diffs), &repo)).unwrap();
                    black_box(&diffs);
                });
                assert_eq!(diffs[0].1.commits.len(), commits);
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn remote_client_control() {
        let body = "{{ commit.remote.username }} {{ commit.remote.pr_number }}";
        let req = client_request(Some(body));
        let project = project(&req);
        let updater = Updater { project: &project, req: &req };
        let repo = Repo::new(project.root()).unwrap();
        let package: Package = fake_package::FakePackage::new("package").into();
        let mut diff = Diff::new(true);
        // A synthetic dependency-update commit requires a client for a remote template
        // but never requests information about its dummy ID from the network.
        diff.commits.push(Commit::new(NO_COMMIT_ID.to_owned(), "chore: update dependencies".into()));
        let actual = updater.fill_commits(vec![(&package, diff.clone())], &repo).await.unwrap();
        assert_eq!(actual[0].1.commits, diff.commits);
        let req = request().with_changelog_req(req.changelog_req().clone());
        let updater = Updater { project: &project, req: &req };
        let error = updater.fill_commits(vec![(&package, diff)], &repo).await.unwrap_err();
        assert!(format!("{error:#}").contains("git token wasn't provided"));
    }
