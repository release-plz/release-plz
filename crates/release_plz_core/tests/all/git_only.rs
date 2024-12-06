use crate::helpers::git_only_test::{GitOnlyTestContext, PROJECT_NAME};
use cargo_metadata::semver::Version;
use cargo_utils::{LocalManifest, CARGO_TOML};
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
            assert_eq!(updates.len(), packages.len());

            for ((name, old_version, new_version), (package, result)) in packages.zip(updates) {
                assert_eq!(package.name, name);
                assert_eq!(package.version, old_version);
                assert_eq!(result.version, new_version);
            }
        }
    }
}

#[tokio::test]
async fn single_crate() {
    let context = GitOnlyTestContext::new().await;

    context
        .run_update()
        .await
        .expect("initial update should succeed")
        .assert_packages_updated([(PROJECT_NAME, Version::new(0, 1, 0), Version::new(0, 1, 0))]);

    context.add_all_commit_and_push("chore: release");

    context
        .run_release()
        .await
        .expect("initial release should succeed")
        .expect("initial release should not be empty");

    touch(context.project_dir().join("included")).unwrap();
    context.add_all_commit_and_push("fix: Add `included` file");

    // Add package excludes
    let mut cargo_toml = LocalManifest::try_new(&context.project_dir().join(CARGO_TOML)).unwrap();
    const EXCLUDED_FILENAME: &'static str = "excluded";
    cargo_toml.data["package"]["exclude"] =
        toml_edit::Item::from(toml_edit::Array::from_iter([EXCLUDED_FILENAME]));
    cargo_toml.write().unwrap();

    context.add_all_commit_and_push("fix: Exclude `excluded` from package");

    context
        .run_update()
        .await
        .expect("second update should succeed")
        .assert_packages_updated([(PROJECT_NAME, Version::new(0, 1, 0), Version::new(0, 1, 1))]);

    context.add_all_commit_and_push("chore: release");

    context
        .run_release()
        .await
        .expect("second release should succeed")
        .expect("second release should not be empty");

    touch(context.project_dir().join(EXCLUDED_FILENAME)).unwrap();
    context.add_all_commit_and_push("chore: Add `excluded` file");

    // Modifying file excluded from package should not lead to version increment
    context
        .run_update()
        .await
        .expect("update should succeed")
        .assert_packages_updated([]);

    assert!(context.repo.is_clean().is_ok());
}
