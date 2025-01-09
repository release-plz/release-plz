use crate::helpers::git_only_test::{GitOnlyTestContext, TestOptions};
use cargo_metadata::camino::{Utf8Component, Utf8PathBuf};
use cargo_metadata::semver::Version;
use release_plz_core::PackagesUpdate;
use std::path::PathBuf;

fn touch(path: impl Into<PathBuf>) -> std::io::Result<()> {
    fs_err::OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .map(|_| ())
}

trait AssertUpdate {
    fn assert_packages_updated<'s>(
        &self,
        packages: impl IntoIterator<IntoIter: ExactSizeIterator<Item = (&'s str, Version, Version)>>,
    );
}

impl AssertUpdate for PackagesUpdate {
    fn assert_packages_updated<'s>(
        &self,
        packages: impl IntoIterator<IntoIter: ExactSizeIterator<Item = (&'s str, Version, Version)>>,
    ) {
        {
            let packages = packages.into_iter();
            let updates = self.updates();
            assert_eq!(
                updates.len(),
                packages.len(),
                "expected {} packages updated, got {}",
                packages.len(),
                updates.len()
            );

            for ((name, old_version, new_version), (package, result)) in packages.zip(updates) {
                assert_eq!(
                    package.name, name,
                    "expected package name {}, got {}",
                    name, package.name
                );
                assert_eq!(
                    package.version, old_version,
                    "expected package version {}, got {}",
                    old_version, package.version
                );
                assert_eq!(
                    result.version, new_version,
                    "expected new version {}, got {}",
                    new_version, package.version
                );
            }
        }
    }
}

#[tokio::test]
async fn single_crate() {
    let workspace_name = "myworkspace";
    let workspace_subdirectory = Utf8PathBuf::from(workspace_name);
    let relative_manifest_path = workspace_subdirectory.join(cargo_utils::CARGO_TOML);
    let options = TestOptions {
        workspace_subdirectory: Some(workspace_subdirectory),
        ..Default::default()
    };
    let context = GitOnlyTestContext::new(options).await;

    context
        .run_update_and_commit()
        .await
        .expect("initial update should succeed")
        .assert_packages_updated([(workspace_name, Version::new(0, 1, 0), Version::new(0, 1, 0))]);

    context
        .run_release()
        .await
        .expect("initial release should succeed")
        .expect("initial release should not be empty");

    touch(context.workspace_dir().join("included")).unwrap();
    context.add_all_commit_and_push("fix: Add `included` file");

    // Create misleading tag v9.8.7 that does not actually point to a version 9.8.7 of the package
    // If not correctly handled, we'll get the following error:
    //  package `myworkspace` has a different version (0.1.1) with respect to the
    //  registry package (0.1.0), but the git tag v0.1.1 exists.
    // This is because the published package selection mechanism would use this tag as it
    // is the latest "version tag" even though it doesn't actually correspond to version 9.8.7
    // of the package
    context
        .repo
        .tag("v9.8.7", "random version tag")
        .expect("git tag should succeed");

    // Add package excludes
    const EXCLUDED_FILENAME: &str = "excluded";
    context
        .write_root_cargo_toml(|cargo_toml| {
            cargo_toml["package"]["exclude"] =
                toml_edit::Array::from_iter([EXCLUDED_FILENAME]).into();
        })
        .unwrap();

    context.add_all_commit_and_push("fix: Exclude `excluded` from package");

    context
        .run_update_and_commit()
        .await
        .expect("second update should succeed")
        .assert_packages_updated([(workspace_name, Version::new(0, 1, 0), Version::new(0, 1, 1))]);

    context
        .run_release()
        .await
        .expect("second release should succeed")
        .expect("second release should not be empty");

    touch(context.workspace_dir().join(EXCLUDED_FILENAME)).unwrap();
    context.add_all_commit_and_push("chore: Add `excluded` file");

    // Modifying file excluded from package should not lead to version increment
    context
        .run_update()
        .await
        .expect("update should succeed")
        .assert_packages_updated([]);

    assert!(context.repo.is_clean().is_ok());

    // Test non-content change to Cargo.toml
    // Since the contents are unchanged, the version should remain unchanged.
    // We do this by setting the executable bit to the file
    context
        .repo
        .git(&[
            "add",
            "--chmod",
            "+x",
            "--",
            relative_manifest_path.as_str(),
        ])
        .unwrap();
    context
        .repo
        .commit("chore: Make Cargo.toml executable")
        .unwrap();
    // If the filesystem supports an executable bit, set it on the file by restoring it from the index
    context
        .repo
        .git(&["restore", "--worktree", relative_manifest_path.as_str()])
        .unwrap();

    context
        .run_update()
        .await
        .expect("update should succeed")
        .assert_packages_updated([]);

    assert!(context.repo.is_clean().is_ok());

    // Test README
    let readme_path = context.workspace_dir().join("README.md");
    fs_err::write(&readme_path, "README").unwrap();

    context.add_all_commit_and_push("fix: Add README");

    context
        .run_update_and_commit()
        .await
        .expect("update after adding README should succeed")
        .assert_packages_updated([(workspace_name, Version::new(0, 1, 1), Version::new(0, 1, 2))]);

    context
        .run_release()
        .await
        .expect("release should succeed")
        .expect("release should not be empty");

    fs_err::write(&readme_path, "README modified").unwrap();

    context.add_all_commit_and_push("chore: Modify README");

    context
        .run_update_and_commit()
        .await
        .expect("update after modifying README should succeed")
        .assert_packages_updated([(workspace_name, Version::new(0, 1, 2), Version::new(0, 1, 3))]);

    context
        .run_release()
        .await
        .expect("release should succeed")
        .expect("release should not be empty");
}

#[tokio::test]
async fn workspace() {
    let options = TestOptions {
        tag_template: Some("{{ package }}--vv{{ version }}".into()),
        ..Default::default()
    };

    let (context, crates) =
        GitOnlyTestContext::new_workspace(options, ["publish-false", "dependant", "other"]).await;
    let [publish_false_crate, dependant_crate, other_crate] = &crates;
    let crate_names = crates.each_ref().map(|dir| dir.file_name().unwrap());
    let [publish_false_crate_name, dependant_crate_name, other_crate_name] = crate_names;

    context
        .run_update_and_commit()
        .await
        .expect("initial update should succeed")
        .assert_packages_updated(
            crate_names
                .each_ref()
                .map(|&name| (name, Version::new(0, 1, 0), Version::new(0, 1, 0))),
        );

    context
        .run_release()
        .await
        .expect("initial release should succeed")
        .expect("initial release should not be empty");

    // Write publish = false in Cargo.toml
    context
        .write_cargo_toml(publish_false_crate, |cargo_toml| {
            cargo_toml["package"]["publish"] = false.into();
        })
        .unwrap();
    context.add_all_commit_and_push("fix(publish-false): Set package.publish = false");

    context
        .run_update_and_commit()
        .await
        .expect("publish-false update should succeed")
        .assert_packages_updated([(
            publish_false_crate_name,
            Version::new(0, 1, 0),
            Version::new(0, 1, 1),
        )]);

    context
        .run_release()
        .await
        .expect("publish-false release should succeed")
        .expect("publish-false release should not be empty");

    // Make 'dependant' depend on 'publish-false'
    // Simulates a binary depending on an unpublished library in the same workspace

    context
        .write_cargo_toml(dependant_crate, |cargo_toml| {
            // Relative path to a crate is found by going up by number of components in this
            // crate's path (i.e. to the workspace root) and appending the path of the dependant crate
            let relative_path = Utf8PathBuf::from_iter(
                dependant_crate
                    .components()
                    .map(|_| Utf8Component::ParentDir)
                    .chain(publish_false_crate.components()),
            );

            cargo_toml["dependencies"][publish_false_crate_name] =
                toml_edit::InlineTable::from_iter([
                    ("path", relative_path.as_str()),
                    ("version", "0.1.0"),
                ])
                .into();
        })
        .expect("writing Cargo.toml should succeed");

    context.add_all_commit_and_push("feat(dependant)!: Add dependency on 'publish-false'");

    context
        .run_update_and_commit()
        .await
        .expect("crates update should succeed")
        .assert_packages_updated([(
            dependant_crate_name,
            Version::new(0, 1, 0),
            Version::new(0, 2, 0),
        )]);

    // TODO: Check contents of release
    context
        .run_release()
        .await
        .expect("publish-false release should succeed")
        .expect("publish-false release should not be empty");

    // Since 'dependant' has a path dependency on 'publish-false',
    // updating 'publish-false' must trigger an update of 'dependant'

    touch(context.crate_dir(publish_false_crate).join("foo")).unwrap();
    context.add_all_commit_and_push("fix(publish-false): Add foo");

    context
        .run_update_and_commit()
        .await
        .expect("crates update should succeed")
        .assert_packages_updated([
            (
                publish_false_crate_name,
                Version::new(0, 1, 1),
                Version::new(0, 1, 2),
            ),
            (
                dependant_crate_name,
                Version::new(0, 2, 0),
                Version::new(0, 2, 1),
            ),
        ]);

    // Test Cargo.lock update
    // Add `rand` dependency to `other`
    context
        .write_cargo_toml(other_crate, |cargo_toml| {
            cargo_toml["dependencies"]["rand"] = "0.8.0".into();
        })
        .unwrap();

    // Run cargo update to update lockfile
    context
        .run_cargo_update(vec![], None)
        .expect("update lockfile should not fail");

    // Set rand to exactly version 0.8.0
    context
        .run_cargo_update(vec!["rand".into()], Some("0.8.0"))
        .expect("update lockfile should not fail");

    context.add_all_commit_and_push("feat(other)!: Add dependency on rand crate");

    context
        .run_update_and_commit()
        .await
        .expect("update after adding rand dependency should succeed")
        .assert_packages_updated([(
            other_crate_name,
            Version::new(0, 1, 0),
            Version::new(0, 2, 0),
        )]);

    context
        .run_release()
        .await
        .expect("release should succeed")
        .expect("release should not be empty");

    // Now run cargo update again, but this time update everything
    // This should update rand to a newer 0.8.x version in the lockfile
    // Which should trigger a version bump for *only* the `other` crate
    context
        .run_cargo_update(vec![], None)
        .expect("update lockfile should not fail");

    context.add_all_commit_and_push("chore: Run cargo update");

    context
        .run_update_and_commit()
        .await
        .expect("update after running `cargo update` should succeed")
        .assert_packages_updated([(
            other_crate_name,
            Version::new(0, 2, 0),
            Version::new(0, 2, 1),
        )]);

    context
        .run_release()
        .await
        .expect("release should succeed")
        .expect("release should not be empty");
}
