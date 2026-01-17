use regex::Regex;
use semver::Version;

use crate::VersionIncrement;

/// This struct allows to increment a version by
/// specifying a configuration.
///
/// Useful if you don't like the default increment rules of the crate.
///
/// # Example
///
/// ```
/// use next_version::VersionUpdater;
/// use semver::Version;
///
/// let updated_version = VersionUpdater::new()
///     .with_features_always_increment_minor(false)
///     .with_breaking_always_increment_major(true)
///     .increment(&Version::new(1, 2, 3), ["feat: commit 1", "fix: commit 2"]);
///
/// assert_eq!(Version::new(1, 3, 0), updated_version);
/// ```
#[derive(Debug)]
pub struct VersionUpdater {
    pub(crate) features_always_increment_minor: bool,
    pub(crate) breaking_always_increment_major: bool,
    pub(crate) custom_major_increment_regex: Option<Regex>,
    pub(crate) custom_minor_increment_regex: Option<Regex>,
}

impl Default for VersionUpdater {
    fn default() -> Self {
        Self::new()
    }
}

impl VersionUpdater {
    /// Constructs a new instance with the default rules of the crate.
    ///
    /// If you don't customize the struct further, it is equivalent to
    /// calling [`crate::NextVersion::next`].
    ///
    /// ```
    /// use next_version::{NextVersion, VersionUpdater};
    /// use semver::Version;
    ///
    /// let version = Version::new(1, 2, 3);
    /// let commits = ["feat: commit 1", "fix: commit 2"];
    /// let updated_version1 = VersionUpdater::new()
    ///     .increment(&version, &commits);
    /// let updated_version2 = version.next(&commits);
    ///
    /// assert_eq!(updated_version1, updated_version2);
    /// ```
    pub fn new() -> Self {
        Self {
            features_always_increment_minor: false,
            breaking_always_increment_major: false,
            custom_major_increment_regex: None,
            custom_minor_increment_regex: None,
        }
    }

    /// Configures automatic minor version increments for feature changes.
    ///
    /// - When `true` is passed, a feature will always trigger a minor version update.
    /// - When `false` is passed, a feature will trigger:
    ///   - a patch version update if the major version is 0.
    ///   - a minor version update otherwise.
    ///
    /// Default: `false`.
    ///
    /// ```rust
    /// use semver::Version;
    /// use next_version::VersionUpdater;
    ///
    /// let commits = ["feat: make coffee"];
    /// let version = Version::new(0, 2, 3);
    /// assert_eq!(
    ///     VersionUpdater::new()
    ///         .with_features_always_increment_minor(true)
    ///         .increment(&version, &commits),
    ///     Version::new(0, 3, 0)
    /// );
    /// assert_eq!(
    ///     VersionUpdater::new()
    ///         .increment(&version, &commits),
    ///     Version::new(0, 2, 4)
    /// );
    /// ```
    pub fn with_features_always_increment_minor(
        mut self,
        features_always_increment_minor: bool,
    ) -> Self {
        self.features_always_increment_minor = features_always_increment_minor;
        self
    }

    /// Configures `0 -> 1` major version increments for breaking changes.
    ///
    /// - When `true` is passed, a breaking change commit will always trigger a major version update
    ///   (including the transition from version 0 to 1)
    /// - When `false` is passed, a breaking change commit will trigger:
    ///   - a minor version update if the major version is 0.
    ///   - a major version update otherwise.
    ///
    /// Default: `false`.
    ///
    /// ```rust
    /// use semver::Version;
    /// use next_version::VersionUpdater;
    ///
    /// let commits = ["feat!: incompatible change"];
    /// let version = Version::new(0, 2, 3);
    /// assert_eq!(
    ///     VersionUpdater::new()
    ///         .with_breaking_always_increment_major(true)
    ///         .increment(&version, &commits),
    ///     Version::new(1, 0, 0)
    /// );
    /// assert_eq!(
    ///     VersionUpdater::new()
    ///         .increment(&version, &commits),
    ///     Version::new(0, 3, 0)
    /// );
    /// ```
    pub fn with_breaking_always_increment_major(
        mut self,
        breaking_always_increment_major: bool,
    ) -> Self {
        self.breaking_always_increment_major = breaking_always_increment_major;
        self
    }

    /// Configure a custom regex pattern for major version increments.
    ///
    /// - For conventional commits, this will check only the type of the commit against the given pattern.
    /// - For non-conventional commits, this will check the entire commit message against the given pattern.
    ///   If you want to match only the beginning of the commit message, use `^` at the start of your regex.
    ///
    /// Default: `None`.
    ///
    /// ### Note
    /// `commit type` according to the spec is only `[a-zA-Z]+`
    ///
    /// ### Example
    ///
    /// ```rust
    /// use semver::Version;
    /// use next_version::VersionUpdater;
    ///
    /// let commits = ["abc: incompatible change"];
    /// let version = Version::new(1, 2, 3);
    /// assert_eq!(
    ///     VersionUpdater::new()
    ///         .with_custom_major_increment_regex("abc")
    ///         .expect("invalid regex")
    ///         .increment(&version, &commits),
    ///     Version::new(2, 0, 0)
    /// );
    /// assert_eq!(
    ///     VersionUpdater::new()
    ///         .increment(&version, &commits),
    ///     Version::new(1, 2, 4)
    /// );
    /// ```
    ///
    /// ### Overriding default behavior
    ///
    /// You can also override the default behavior of conventional commits types.
    /// For example, you can make a `feat` commit trigger a _major_ version increment
    /// instead of _minor_:
    ///
    /// ```rust
    /// use semver::Version;
    /// use next_version::VersionUpdater;
    ///
    /// let commits = ["feat: incompatible change"];
    /// let version = Version::new(1, 2, 3);
    /// assert_eq!(
    ///     VersionUpdater::new()
    ///         .with_custom_major_increment_regex("feat")
    ///         .expect("invalid regex")
    ///         .increment(&version, &commits),
    ///     Version::new(2, 0, 0)
    /// );
    /// assert_eq!(
    ///     VersionUpdater::new()
    ///         .increment(&version, &commits),
    ///     Version::new(1, 3, 0)
    /// );
    /// ```
    pub fn with_custom_major_increment_regex(
        mut self,
        custom_major_increment_regex: &str,
    ) -> Result<Self, regex::Error> {
        let regex = Regex::new(custom_major_increment_regex)?;
        self.custom_major_increment_regex = Some(regex);
        Ok(self)
    }

    /// Configures a custom regex pattern for minor version increments.
    ///
    /// - For conventional commits, this will check only the type of the commit against the given pattern.
    /// - For non-conventional commits, this will check the entire commit message against the given pattern.
    ///   If you want to match only the beginning of the commit message, use `^` at the start of your regex.
    ///
    /// Default: `None`.
    ///
    /// ### Note
    /// `commit type` according to the spec is only `[a-zA-Z]+`
    ///
    /// ### Example
    ///
    /// ```rust
    /// use semver::Version;
    /// use next_version::VersionUpdater;
    ///
    /// let commits = ["bbb: make coffee"];
    /// let version = Version::new(0, 2, 3);
    /// assert_eq!(
    ///     VersionUpdater::new()
    ///         .with_custom_minor_increment_regex("^abc|^bbb")
    ///         .expect("invalid regex")
    ///         .increment(&version, &commits),
    ///     Version::new(0, 3, 0)
    /// );
    /// assert_eq!(
    ///     VersionUpdater::new()
    ///         .increment(&version, &commits),
    ///     Version::new(0, 2, 4)
    /// );
    /// ```
    pub fn with_custom_minor_increment_regex(
        mut self,
        custom_minor_increment_regex: &str,
    ) -> Result<Self, regex::Error> {
        let regex = Regex::new(custom_minor_increment_regex)?;
        self.custom_minor_increment_regex = Some(regex);
        Ok(self)
    }

    /// Analyze commits and determine the next version.
    pub fn increment<I>(self, version: &Version, commits: I) -> Version
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let increment = VersionIncrement::from_commits_with_updater(&self, version, commits);
        match increment {
            Some(increment) => increment.bump(version),
            None => version.clone(),
        }
    }
}
