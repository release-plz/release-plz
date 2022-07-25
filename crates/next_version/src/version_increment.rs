use conventional_commit_parser::commit::{CommitType, ConventionalCommit};
use semver::Version;

pub enum VersionIncrement {
    Major,
    Minor,
    Patch,
}

impl VersionIncrement {
    /// Analyze commits and determine which part of version to increment based on
    /// [conventional commits](https://www.conventionalcommits.org/) and
    /// [Semantic versioning](https://semver.org/).
    /// - If no commits are present, [`Option::None`] is returned, because the version should not be incremented.
    /// - If some commits are present, but none of them match conventional commits specification,
    ///   the version increment is [`VersionIncrement::Patch`].
    /// - If some commits match conventional commits, then the next version is calculated by using
    ///   [these](https://www.conventionalcommits.org/en/v1.0.0/#how-does-this-relate-to-semverare) rules.
    pub fn from_commits<I>(current_version: &Version, commits: I) -> Option<Self>
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let mut commits = commits.into_iter().peekable();
        let are_commits_present = commits.peek().is_some();
        if !are_commits_present {
            None
        } else {
            // Parse commits and keep only the ones that follow conventional commits specification.
            let commits: Vec<ConventionalCommit> = commits
                .filter_map(|c| conventional_commit_parser::parse(c.as_ref()).ok())
                .collect();

            Some(Self::from_conventional_commits(current_version, &commits))
        }
    }

    /// If no conventional commits are present, the version is incremented as a Patch
    fn from_conventional_commits(
        current_version: &Version,
        commits: &[ConventionalCommit],
    ) -> Self {
        let is_there_a_breaking_change = commits.iter().any(|commit| commit.is_breaking_change);

        let is_major_bump = || current_version.major != 0 && is_there_a_breaking_change;

        let is_minor_bump = || {
            current_version.major != 0
                && commits
                    .iter()
                    .any(|commit| commit.commit_type == CommitType::Feature)
                || current_version.major == 0 && is_there_a_breaking_change
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
        // This code is essentially the inlined version of these functions from cargo-edit
        // increment_major: https://github.com/killercup/cargo-edit/blob/643e9253a84db02c52a7fa94f07d786d281362ab/src/version.rs#L38
        // increment_minor: https://github.com/killercup/cargo-edit/blob/643e9253a84db02c52a7fa94f07d786d281362ab/src/version.rs#L46
        // increment_patch: https://github.com/killercup/cargo-edit/blob/643e9253a84db02c52a7fa94f07d786d281362ab/src/version.rs#L53
        match self {
            Self::Major => Version {
                major: version.major + 1,
                minor: 0,
                patch: 0,
                pre: semver::Prerelease::EMPTY,
                build: semver::BuildMetadata::EMPTY,
            },
            Self::Minor => Version {
                major: version.major,
                minor: version.minor + 1,
                patch: 0,
                pre: semver::Prerelease::EMPTY,
                build: semver::BuildMetadata::EMPTY,
            },
            Self::Patch => Version {
                major: version.major,
                minor: version.minor,
                patch: version.patch + 1,
                pre: semver::Prerelease::EMPTY,
                build: semver::BuildMetadata::EMPTY,
            },
        }
    }
}
