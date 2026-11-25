use semver::Version;

use crate::VersionUpdater;

pub trait NextVersion {
    #[must_use]
    fn next<I>(&self, commits: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<str>;

    /// Increments the major version number.
    #[must_use]
    fn increment_major(&self) -> Self;
    /// Increments the minor version number.
    #[must_use]
    fn increment_minor(&self) -> Self;
    /// Increments the patch version number.
    #[must_use]
    fn increment_patch(&self) -> Self;
    /// Increments the prerelease version number.
    #[must_use]
    fn increment_prerelease(&self) -> Self;
}

impl NextVersion for Version {
    /// Analyze commits and determine the next version based on
    /// [conventional commits](https://www.conventionalcommits.org/) and
    /// [semantic versioning](https://semver.org/):
    /// - If no commits are passed, the version is unchanged.
    /// - If some commits are present, but none of them match conventional commits specification,
    ///   the version is incremented as a Patch.
    /// - If some commits match conventional commits, then the next version is calculated by using
    ///   [these](https://www.conventionalcommits.org/en/v1.0.0/#how-does-this-relate-to-semverare) rules.
    ///
    /// ```rust
    /// use next_version::NextVersion;
    /// use semver::Version;
    ///
    /// let commits = ["feat: make coffe"];
    /// let version = Version::new(0, 3, 3);
    /// assert_eq!(version.next(commits), Version::new(0, 3, 4));
    /// ```
    fn next<I>(&self, commits: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        VersionUpdater::default().increment(self, commits)
    }

    // taken from https://github.com/killercup/cargo-edit/blob/643e9253a84db02c52a7fa94f07d786d281362ab/src/version.rs#L38
    fn increment_major(&self) -> Self {
        Self {
            major: self.major + 1,
            minor: 0,
            patch: 0,
            pre: semver::Prerelease::EMPTY,
            build: self.build.clone(),
        }
    }

    // taken from https://github.com/killercup/cargo-edit/blob/643e9253a84db02c52a7fa94f07d786d281362ab/src/version.rs#L46
    fn increment_minor(&self) -> Self {
        Self {
            minor: self.minor + 1,
            patch: 0,
            pre: semver::Prerelease::EMPTY,
            ..self.clone()
        }
    }

    // taken from https://github.com/killercup/cargo-edit/blob/643e9253a84db02c52a7fa94f07d786d281362ab/src/version.rs#L53
    fn increment_patch(&self) -> Self {
        Self {
            patch: self.patch + 1,
            pre: semver::Prerelease::EMPTY,
            ..self.clone()
        }
    }

    fn increment_prerelease(&self) -> Self {
        let next_pre = increment_last_identifier(self.pre.as_str());
        let next_pre = semver::Prerelease::new(&next_pre).expect("pre release increment failed. Please report this issue to https://github.com/release-plz/release-plz/issues");
        Self {
            pre: next_pre,
            ..self.clone()
        }
    }
}

fn increment_last_identifier(release: &str) -> String {
    let last_start = release.rfind('.').map_or(0, |dot| dot + 1);
    let (prefix, last) = release.split_at(last_start);
    match last.parse::<u32>().ok().and_then(|num| num.checked_add(1)) {
        Some(next) => format!("{prefix}{next}"),
        None => format!("{release}.1"),
    }
}
