    #[test]
    #[ignore]
    fn grouped_custom_rules_control() {
        let package: Package = fake_package::FakePackage::new("package").into();
        let mut diff = Diff::new(true);
        diff.commits.push(Commit::new("1e6903d".into(), "enhancement: add feature".into()));
        for invalid in [false, true] {
            let req = request().with_package_config("package", crate::PackageUpdateConfig {
                generic: crate::UpdateConfig {
                    custom_minor_increment_regex: Some(if invalid { "[" } else { "^enhancement$" }.into()),
                    ..Default::default()
                },
                version_group: Some("group".into()),
                ..Default::default()
            });
            let project = project(&req);
            let updater = Updater { project: &project, req: &req };
            let actual = updater.get_version_groups(&[(&package, diff.clone())]);
            if invalid {
                assert!(actual.is_err());
            } else {
                assert_eq!(actual.unwrap()["group"], Version::new(0, 2, 0));
            }
        }
    }
