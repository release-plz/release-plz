use cargo_metadata::semver::Version;
use next_version::{VersionIncrement, VersionUpdater};

use crate::{diff::Diff, semver_check::SemverCheck};

pub(crate) trait NextVersionFromDiff {
    /// Analyze commits and determine which part of version to increment based on
    /// [conventional commits](https://www.conventionalcommits.org/)
    fn next_from_diff(&self, diff: &Diff, version_updater: VersionUpdater) -> Self;
}

impl NextVersionFromDiff for Version {
    fn next_from_diff(&self, diff: &Diff, version_updater: VersionUpdater) -> Self {
        if !diff.should_update_version() {
            self.clone()
        } else if matches!(diff.semver_check, SemverCheck::Incompatible(_)) {
            let increment = VersionIncrement::breaking(self);
            increment.bump(self)
        } else {
            version_updater.increment(self, diff.commits.iter().map(|c| &c.message))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::diff::Commit;

    use crate::NO_COMMIT_ID;

    use super::*;

    #[test]
    fn next_version_of_new_package_is_unchanged() {
        let registry_package_exists = false;
        let diff = Diff::new(registry_package_exists);
        let version = Version::new(1, 2, 3);
        assert_eq!(
            version
                .clone()
                .next_from_diff(&diff, VersionUpdater::default()),
            version
        );
    }

    #[test]
    fn next_version_of_existing_package_is_updated() {
        let diff = Diff {
            registry_package_exists: true,
            commits: vec![Commit::new(
                NO_COMMIT_ID.to_string(),
                "my change".to_string(),
            )],
            is_version_published: true,
            semver_check: SemverCheck::Skipped,
        };
        let version = Version::new(1, 2, 3);
        assert_eq!(
            version.next_from_diff(&diff, VersionUpdater::default()),
            Version::new(1, 2, 4)
        );
    }

    #[test]
    fn next_version_doesnt_bump_0_x_minor_version_for_features() {
        let diff = Diff {
            registry_package_exists: true,
            commits: vec![Commit::new(
                NO_COMMIT_ID.to_string(),
                "feat: my change".to_string(),
            )],
            is_version_published: true,
            semver_check: SemverCheck::Skipped,
        };
        let version = Version::new(0, 2, 3);
        assert_eq!(
            version.next_from_diff(&diff, VersionUpdater::default()),
            Version::new(0, 2, 4)
        );
    }

    #[test]
    fn next_version_bumps_0_x_minor_version_for_features() {
        let diff = Diff {
            registry_package_exists: true,
            commits: vec![Commit::new(
                NO_COMMIT_ID.to_string(),
                "feat: my change".to_string(),
            )],
            is_version_published: true,
            semver_check: SemverCheck::Skipped,
        };
        let version = Version::new(0, 2, 3);
        let updater = VersionUpdater::default().with_features_always_increment_minor(true);
        assert_eq!(
            version.next_from_diff(&diff, updater),
            Version::new(0, 3, 0)
        );
    }
}
