#[cfg(test)]
mod perf_update_manifests {
    use super::*;
    use crate::{UpdateResult, semver_check::SemverCheck, test_utils::write_package};
    use std::{hint::black_box, time::Instant};

    // Exercise the actual manifest-writing path using the independent-package
    // fixtures from #3146, with planned changelog data retained by the caller.
    #[test]
    #[ignore]
    fn benchmark_update_manifests() {
        for count in [10, 100] {
            let directory = crate::fs_utils::Utf8TempDir::new().unwrap();
            let root = directory.path();
            let manifest = root.join("Cargo.toml");
            fs_err::write(
                &manifest,
                "[workspace]\nmembers = [\"packages/*\"]\nresolver = \"3\"\n",
            )
            .unwrap();
            for index in 0..count {
                write_package(
                    &root.join(format!("packages/package-{index}")),
                    &format!("package-{index}"),
                    "1.0.0",
                    "",
                );
            }
            let metadata = cargo_utils::get_manifest_metadata(&manifest).unwrap();
            let packages: Vec<_> = metadata.workspace_packages();
            for changelog_bytes in [0, 65_536, 524_288] {
                let updates: Vec<_> = [1, 2]
                    .into_iter()
                    .map(|patch| {
                        PackagesUpdate::new(
                            packages
                                .iter()
                                .map(|package| {
                                    (
                                        (**package).clone(),
                                        UpdateResult {
                                            version: Version::new(1, 0, patch),
                                            changelog: Some(
                                                "- A historical changelog entry.\n"
                                                    .repeat(changelog_bytes / 32),
                                            ),
                                            semver_check: SemverCheck::Skipped,
                                            new_changelog_entry: Some(
                                                "## 1.0.x\n- A release.\n".into(),
                                            ),
                                            registry_version: Some(Version::new(1, 0, 0)),
                                        },
                                    )
                                })
                                .collect(),
                        )
                    })
                    .collect();
                for plan in &updates {
                    for (_, update) in plan.updates() {
                        assert_eq!(update.changelog.as_ref().unwrap().len(), changelog_bytes);
                    }
                }
                let run = |index: usize| {
                    update_manifests(
                        black_box(&updates[index % 2]),
                        &manifest,
                        black_box(&packages),
                    )
                    .unwrap()
                };
                for index in 0..10 {
                    run(index);
                }
                let iterations = 100;
                let start = Instant::now();
                for index in 0..iterations {
                    run(index);
                }
                let elapsed = start.elapsed();
                for package in &packages {
                    assert_eq!(LocalManifest::try_new(&package.manifest_path).unwrap().data["package"]["version"].as_str(), Some("1.0.2"));
                }
                println!(
                    "update_manifests/{count}_packages/{changelog_bytes}_changelog_bytes: {:.3} us/iteration ({iterations} iterations)",
                    elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64
                );
            }
        }
    }
}
