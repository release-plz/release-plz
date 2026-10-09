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
        let breaking_increment_major =
            current_version.major != 0 || updater.breaking_always_increment_major;
        let features_increment_minor =
            current_version.major != 0 || updater.features_always_increment_minor;
        let mut increment = None;

        for msg in commits {
            let msg = msg.as_ref();
            let commit = Commit::parse(msg).ok();
            // Custom regexes match conventional commit types, or the whole message otherwise.
            let regex_input = commit
                .as_ref()
                .map_or(msg, |commit| commit.type_().as_str());
            let matches =
                |regex: Option<&Regex>| regex.is_some_and(|regex| regex.is_match(regex_input));
            if matches(updater.no_increment_regex.as_ref()) {
                continue;
            }
            if !current_version.pre.is_empty() {
                return Some(Self::Prerelease);
            }
            increment.get_or_insert(Self::Patch);

            if let Some(commit) = &commit
                && commit.breaking()
            {
                if breaking_increment_major {
                    // No other commit can request a larger increment.
                    return Some(Self::Major);
                }
                if current_version.minor != 0 {
                    increment = Some(Self::Minor);
                }
                // In 0.0.x, breaking changes only increment the patch,
                // but this or other commits can still request a larger increment.
            }

            if matches(updater.custom_major_increment_regex.as_ref()) {
                return Some(Self::Major);
            }
            if increment != Some(Self::Minor) {
                let is_feature = features_increment_minor
                    && commit
                        .as_ref()
                        .is_some_and(|commit| commit.type_() == git_conventional::Type::FEAT);
                if is_feature || matches(updater.custom_minor_increment_regex.as_ref()) {
                    increment = Some(Self::Minor);
                }
            }
        }

        increment
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
