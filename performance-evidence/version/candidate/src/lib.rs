//! Library to calculate next semantic version based on
//! [conventional commits](https://www.conventionalcommits.org/).
//!
//! It does not analyze git history, the list of commits must be provided by the user.
//!
//! # Version changes
//!
//! ## Non conventional commits
//!
//! If conventional commits are not used, the patch is incremented.
//!
//! ```rust
//! use semver::Version;
//! use next_version::NextVersion;
//!
//! let commits = ["my change"];
//! assert_eq!(Version::new(1, 2, 3).next(commits), Version::new(1, 2, 4));
//! ```
//!
//! ## `0.0.x` versions
//!
//! In `0.0.x` versions the patch is always incremented:
//!
//! ```rust
//! use semver::Version;
//! use next_version::NextVersion;
//!
//! let commits = ["my change"];
//! assert_eq!(Version::new(0, 0, 4).next(&commits), Version::new(0, 0, 5));
//!
//! let commits = ["feat!: break user"];
//! assert_eq!(Version::new(0, 0, 1).next(&commits), Version::new(0, 0, 2));
//! ```
//!
//! <div class="warning">We don't increase the minor version because the bump
//! from <code>0.0.x</code> to <code>0.1.x</code>
//! should be intentional (not automated) because the author communicates an higher level of
//! API stability to the user.</div>
//!
//! ## Features
//!
//! If a feature comment is present:
//! - If the major number is `0`: the patch is incremented.
//! - Otherwise: the minor is incremented.
//!
//! ```rust
//! use semver::Version;
//! use next_version::NextVersion;
//!
//! let commits = ["my change", "feat: make coffe"];
//! assert_eq!(Version::new(1, 2, 4).next(&commits), Version::new(1, 3, 0));
//!
//! assert_eq!(Version::new(0, 2, 4).next(&commits), Version::new(0, 2, 5));
//! ```
//!
//! <div class="warning">When the major number is <code>0</code>,
//! we don't increase the minor version because the bump from <code>0.x.y</code> to <code>0.(x+1).0</code>
//! indicates a breaking change.</div>
//!
//! ## Breaking changes
//!
//! Breaking changes will increment:
//! - major if major is not `0`.
//! - minor if major is `0`.
//!
//! ```rust
//! use semver::Version;
//! use next_version::NextVersion;
//!
//! let commits = ["feat!: break user"];
//! assert_eq!(Version::new(1, 2, 4).next(&commits), Version::new(2, 0, 0));
//!
//! assert_eq!(Version::new(0, 4, 4).next(&commits), Version::new(0, 5, 0));
//! ```
//!
//! According to the [conventional commits specification](https://www.conventionalcommits.org/),
//! breaking changes can also be specified in the footer:
//!
//! ```rust
//! use semver::Version;
//! use next_version::NextVersion;
//!
//! let breaking_commit = r#"feat: make coffe
//!
//! my change
//!
//! BREAKING CHANGE: user will be broken
//! "#;
//!
//! let commits = [breaking_commit];
//! assert_eq!(Version::new(1, 2, 4).next(&commits), Version::new(2, 0, 0));
//! ```
//!
//! ## Pre-release
//!
//! Pre-release versions are incremented in the same way, independently
//! by the type of commits:
//!
//! ```rust
//! use semver::Version;
//! use next_version::NextVersion;
//!
//! let commits = ["feat!: break user"];
//! let version = Version::parse("1.0.0-alpha.1.2").unwrap();
//! let expected = Version::parse("1.0.0-alpha.1.3").unwrap();
//! assert_eq!(version.next(commits.clone()), expected);
//!
//! // If the pre-release doesn't contain a version, `.1` is appended.
//! let version = Version::parse("1.0.0-beta").unwrap();
//! let expected = Version::parse("1.0.0-beta.1").unwrap();
//! assert_eq!(version.next(commits), expected);
//!
//! ```
//!
//! ## Build metadata
//!
//! Build metadata isn't modified.
//!
//! ```rust
//! use semver::Version;
//! use next_version::NextVersion;
//!
//! let commits = ["my change"];
//! let version = Version::parse("1.0.0-beta.1+1.1.0").unwrap();
//! let expected = Version::parse("1.0.0-beta.2+1.1.0").unwrap();
//! assert_eq!(version.next(commits.clone()), expected);
//!
//! let version = Version::parse("1.0.0+abcd").unwrap();
//! let expected = Version::parse("1.0.1+abcd").unwrap();
//! assert_eq!(version.next(commits.clone()), expected);
//! ```
//!
//! # Custom version increment
//!
//! If you don't like the default increment rules of the crate,
//! you can customize them by using [`VersionUpdater`].

mod next_version;
mod version_increment;
mod version_updater;

pub use crate::{next_version::*, version_increment::*, version_updater::*};
