use git_conventional::Commit;
use regex::Regex;
use semver::Version;

use crate::{NextVersion, VersionUpdater};

#[derive(Debug, PartialEq, Eq)]
pub enum VersionIncrement {
    Major,
    Minor,
    Patch,
    Prerelease,
}

/// Checks if any commit matches the custom regex.
/// - For conventional commits: checks only the commit type
/// - For non-conventional commits: checks the entire message
fn is_there_a_custom_match(
    regex: Option<&Regex>,
    conventional_commits: &[Commit],
    non_conventional_messages: &[&str],
) -> bool {
    regex.is_some_and(|r| {
        // Check conventional commit types
        let matches_type = || {
            conventional_commits
                .iter()
                .any(|commit| r.is_match(commit.type_().as_str()))
        };

        // Check non-conventional commit messages
        let matches_message = || non_conventional_messages.iter().any(|msg| r.is_match(msg));

        matches_type() || matches_message()
    })
}

impl VersionIncrement {
    /// Analyze commits and determine which part of version to increment based on
    /// [conventional commits](https://www.conventionalcommits.org/) and
    /// [Semantic versioning](https://semver.org/).
    /// - If no commits are present, [`Option::None`] is returned, because the version should not be incremented.
    /// - If some commits are present and [`semver::Prerelease`] is not empty, the version increment is
    ///   [`VersionIncrement::Prerelease`].
    /// - If some commits are present, but none of them match conventional commits specification,
    ///   the version increment is [`VersionIncrement::Patch`].
    /// - If some commits match conventional commits, then the next version is calculated by using
    ///   [these](https://www.conventionalcommits.org/en/v1.0.0/#how-does-this-relate-to-semverare) rules.
    pub fn from_commits<I>(current_version: &Version, commits: I) -> Option<Self>
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let updater = VersionUpdater::default();
        Self::from_commits_with_updater(&updater, current_version, commits)
    }

    pub(crate) fn from_commits_with_updater<I>(
        updater: &VersionUpdater,
        current_version: &Version,
        commits: I,
    ) -> Option<Self>
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let mut commits = commits.into_iter().peekable();
        let are_commits_present = commits.peek().is_some();
        if are_commits_present {
            if !current_version.pre.is_empty() {
                return Some(Self::Prerelease);
            }
            // Parse commits and keep only the ones that follow conventional commits specification.
            let commit_messages: Vec<String> = commits.map(|c| c.as_ref().to_string()).collect();

            Some(Self::from_conventional_commits(
                current_version,
                &commit_messages,
                updater,
            ))
        } else {
            None
        }
    }

    /// Increments the version to take into account breaking changes.
    /// ```rust
    /// use next_version::VersionIncrement;
    /// use semver::Version;
    ///
    /// let increment = VersionIncrement::breaking(&Version::new(0, 3, 3));
    /// assert_eq!(increment, VersionIncrement::Minor);
    ///
    /// let increment = VersionIncrement::breaking(&Version::new(1, 3, 3));
    /// assert_eq!(increment, VersionIncrement::Major);
    ///
    /// let increment = VersionIncrement::breaking(&Version::parse("1.3.3-alpha.1").unwrap());
    /// assert_eq!(increment, VersionIncrement::Prerelease);
    /// ```
    pub fn breaking(current_version: &Version) -> Self {
        if !current_version.pre.is_empty() {
            Self::Prerelease
        } else if current_version.major == 0 && current_version.minor == 0 {
            Self::Patch
        } else if current_version.major == 0 {
            Self::Minor
        } else {
            Self::Major
        }
    }

    /// If no conventional commits are present, the version is incremented as a Patch
    fn from_conventional_commits(
        current: &Version,
        commit_messages: &[String],
        updater: &VersionUpdater,
    ) -> Self {
        let mut conventional_commits = Vec::new();
        let mut non_conventional_messages = Vec::new();

        for msg in commit_messages {
            match Commit::parse(msg) {
                Ok(commit) => conventional_commits.push(commit),
                Err(_) => non_conventional_messages.push(msg.as_str()),
            }
        }

        let is_there_a_feature = || {
            conventional_commits
                .iter()
                .any(|commit| commit.type_() == git_conventional::Type::FEAT)
        };

        let is_there_a_breaking_change =
            conventional_commits.iter().any(|commit| commit.breaking());

        let is_major_bump = || {
            (is_there_a_breaking_change
                || is_there_a_custom_match(
                    updater.custom_major_increment_regex.as_ref(),
                    &conventional_commits,
                    &non_conventional_messages,
                ))
                && (current.major != 0 || updater.breaking_always_increment_major)
        };

        let is_minor_bump = || {
            let is_feat_bump = || {
                is_there_a_feature()
                    && (current.major != 0 || updater.features_always_increment_minor)
            };
            let is_breaking_bump =
                || current.major == 0 && current.minor != 0 && is_there_a_breaking_change;
            is_feat_bump()
                || is_breaking_bump()
                || is_there_a_custom_match(
                    updater.custom_minor_increment_regex.as_ref(),
                    &conventional_commits,
                    &non_conventional_messages,
                )
        };

        if is_major_bump() {
            Self::Major
        } else if is_minor_bump() {
            Self::Minor
        } else {
            Self::Patch
        }
    }
}

impl VersionIncrement {
    pub fn bump(&self, version: &Version) -> Version {
        match self {
            Self::Major => version.increment_major(),
            Self::Minor => version.increment_minor(),
            Self::Patch => version.increment_patch(),
            Self::Prerelease => version.increment_prerelease(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to test `is_there_a_custom_match` with a list of commit messages.
    /// Automatically separates conventional and non-conventional commits.
    fn check_custom_match(pattern: &str, messages: &[&str]) -> bool {
        let regex = Regex::new(pattern).unwrap();
        let conventional: Vec<Commit> = messages
            .iter()
            .filter_map(|m| Commit::parse(m).ok())
            .collect();
        let non_conventional: Vec<&str> = messages
            .iter()
            .filter(|m| Commit::parse(m).is_err())
            .copied()
            .collect();
        is_there_a_custom_match(Some(&regex), &conventional, &non_conventional)
    }

    #[test]
    fn returns_true_for_matching_conventional_commit_type() {
        assert!(check_custom_match(r"custom", &["custom: A custom commit"]));
    }

    #[test]
    fn returns_false_for_conventional_commit_with_matching_description_but_not_type() {
        // The regex matches something in the description, but not the type
        // Should NOT match because for conventional commits we only check the type
        assert!(!check_custom_match(r"custom", &["feat: A custom feature"]));
    }

    #[test]
    fn returns_true_for_matching_non_conventional_commit() {
        assert!(check_custom_match(
            r"custom",
            &["A non-conventional commit with custom keyword"]
        ));
    }

    #[test]
    fn returns_false_for_empty_commits_list() {
        assert!(!check_custom_match(r"custom", &[]));
    }
}
