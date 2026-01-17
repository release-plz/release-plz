use cargo_metadata::{Package, semver::Version};
use tracing::warn;

use crate::{UpdateResult, semver_check::SemverCheck};

use super::ReleaseInfo;

pub type PackagesToUpdate = Vec<(Package, UpdateResult)>;

#[derive(Clone, Debug, Default)]
pub struct PackagesUpdate {
    updates: PackagesToUpdate,
    /// New workspace version. If None, the workspace version is not updated.
    /// See cargo [docs](https://doc.rust-lang.org/cargo/reference/workspaces.html#root-package).
    workspace_version: Option<Version>,
}

impl PackagesUpdate {
    pub fn new(updates: PackagesToUpdate) -> Self {
        Self {
            updates,
            workspace_version: None,
        }
    }

    pub fn with_workspace_version(&mut self, workspace_version: Version) {
        self.workspace_version = Some(workspace_version);
    }

    pub fn updates(&self) -> &[(Package, UpdateResult)] {
        &self.updates
    }

    pub fn updates_clone(&self) -> PackagesToUpdate {
        self.updates.clone()
    }

    pub fn updates_mut(&mut self) -> &mut PackagesToUpdate {
        &mut self.updates
    }

    pub fn workspace_version(&self) -> Option<&Version> {
        self.workspace_version.as_ref()
    }

    pub fn summary(&self) -> String {
        let updates = self.updates_summary();
        let breaking_changes = self.breaking_changes();
        format!("{updates}\n{breaking_changes}")
    }

    fn updates_summary(&self) -> String {
        self.updates
            .iter()
            .map(|(package, update)| {
                // Use registry_version as previous_version when available
                // (version already bumped case), otherwise use package.version
                let previous_version = update.registry_version.as_ref().unwrap_or(&package.version);
                if previous_version == &update.version {
                    format!("\n* `{}`: {}", package.name, update.version)
                } else {
                    format!(
                        "\n* `{}`: {} -> {}{}",
                        package.name,
                        previous_version,
                        update.version,
                        update.semver_check.outcome_str()
                    )
                }
            })
            .collect()
    }

    pub fn breaking_changes(&self) -> String {
        self.updates
            .iter()
            .map(|(package, update)| match &update.semver_check {
                SemverCheck::Incompatible(incompatibilities) => {
                    format!(
                        "\n### ⚠️ `{}` breaking changes\n\n```{}```\n",
                        package.name, incompatibilities
                    )
                }
                SemverCheck::Compatible | SemverCheck::Skipped => String::new(),
            })
            .collect()
    }

    /// Return info about releases of the updated packages
    pub fn releases(&self) -> Vec<ReleaseInfo> {
        self.updates
            .iter()
            .map(|(package, update)| {
                let default_result = match &update.new_changelog_entry {
                    Some(entry) => (None, Some(entry.clone())),
                    None => (None, None),
                };
                let (changelog_title, changelog_notes) = match update.last_changes() {
                    Err(e) => {
                        warn!(
                            "can't determine changes in changelog of package {}: {e:?}",
                            package.name
                        );
                        default_result
                    }
                    Ok(Some(c)) => (Some(c.title().to_string()), Some(c.notes().to_string())),
                    Ok(None) => {
                        warn!(
                            "no changes detected in changelog of package {}",
                            package.name
                        );
                        default_result
                    }
                };

                let (semver_check, breaking_changes) = match &update.semver_check {
                    SemverCheck::Incompatible(incompatibilities) => {
                        ("incompatible", Some(incompatibilities.clone()))
                    }
                    SemverCheck::Compatible => ("compatible", None),
                    SemverCheck::Skipped => ("skipped", None),
                };

                // Use registry_version as previous_version when available
                // (version already bumped case), otherwise use package.version
                let previous_version = update
                    .registry_version
                    .as_ref()
                    .unwrap_or(&package.version)
                    .to_string();

                ReleaseInfo {
                    package: package.name.to_string(),
                    title: changelog_title,
                    changelog: changelog_notes,
                    next_version: update.version.to_string(),
                    previous_version,
                    breaking_changes,
                    semver_check: semver_check.to_string(),
                }
            })
            .collect()
    }
}
