use crate::helpers::git_only_test::{GitOnlyTestContext, PROJECT_NAME};
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

    context
        .run_release()
        .await
        .expect("initial release should succeed")
        .expect("initial release should not be empty");

    touch(context.project_dir().join("included")).unwrap();
    context.add_all_commit_and_push("fix: Add `included` file");

    context
        .run_update()
        .await
        .expect("second update should succeed")
        .assert_packages_updated([(PROJECT_NAME, Version::new(0, 1, 0), Version::new(0, 1, 1))]);

    // TODO: Test package excludes
}
