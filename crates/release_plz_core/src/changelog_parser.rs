use anyhow::Context;
use cargo_metadata::camino::Utf8Path;
use regex::Regex;
use std::sync::LazyLock;

/// Parse the header from a changelog.
/// The changelog header is a string at the begin of the changelog that:
/// - Starts with `# Changelog`, `# CHANGELOG`, or `# changelog`
/// - ends with `## Unreleased`, `## [Unreleased]` or `## ..anything..`
///   (in the ..anything.. case, `## ..anything..` is not included in the header)
pub fn parse_header(changelog: &str) -> Option<String> {
    static FIRST_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?s)^(# Changelog|# CHANGELOG|# changelog)(.*)(## Unreleased|## \[Unreleased\]|## unreleased|## \[unreleased\])(.*?)(\n)").unwrap()
    });
    static SECOND_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?s)^(# Changelog|# CHANGELOG|# changelog)(.*?)(\n## )").unwrap()
    });

    if let Some(captures) = FIRST_RE.captures(changelog) {
        return Some(captures[0].to_string());
    }

    if let Some(captures) = SECOND_RE.captures(changelog) {
        return Some(format!("{}{}", &captures[1], &captures[2]));
    }

    None
}

pub fn last_changes(changelog: &Utf8Path) -> anyhow::Result<Option<String>> {
    let changelog = fs_err::read_to_string(changelog).context("can't read changelog file")?;
    last_changes_from_str(&changelog)
}

pub fn last_changes_from_str(changelog: &str) -> anyhow::Result<Option<String>> {
    let parser = ChangelogParser::new(changelog)?;
    let last_release = parser.last_release().map(|r| r.notes.to_string());
    Ok(last_release)
}

pub fn last_version_from_str(changelog: &str) -> anyhow::Result<Option<String>> {
    let parser = ChangelogParser::new(changelog)?;
    let last_release = parser.last_release().map(|r| r.version.to_string());
    Ok(last_release)
}

pub fn last_release_from_str(changelog: &str) -> anyhow::Result<Option<ChangelogRelease>> {
    let parser = ChangelogParser::new(changelog)?;
    let last_release = parser.last_release().map(ChangelogRelease::from_release);
    Ok(last_release)
}

#[derive(Debug)]
pub struct ChangelogRelease {
    title: String,
    notes: String,
}

impl ChangelogRelease {
    fn from_release(release: &parse_changelog::Release) -> Self {
        Self {
            title: release.title.to_string(),
            notes: release.notes.to_string(),
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn notes(&self) -> &str {
        &self.notes
    }
}

pub struct ChangelogParser<'a> {
    changelog: parse_changelog::Changelog<'a>,
}

impl<'a> ChangelogParser<'a> {
    pub fn new(changelog_text: &'a str) -> anyhow::Result<Self> {
        let changelog = parse_changelog::parse(changelog_text).context("can't parse changelog")?;
        Ok(Self { changelog })
    }

    fn last_release(&self) -> Option<&parse_changelog::Release> {
        let last_release = release_at(&self.changelog, 0)?;
        let last_release = if last_release.version.to_lowercase().contains("unreleased") {
            release_at(&self.changelog, 1)?
        } else {
            last_release
        };
        Some(last_release)
    }
}

fn release_at<'a>(
    changelog: &'a parse_changelog::Changelog,
    index: usize,
) -> Option<&'a parse_changelog::Release<'a>> {
    let release = changelog.get_index(index)?.1;
    Some(release)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn last_changes_from_str_test(changelog: &str) -> String {
        last_changes_from_str(changelog).unwrap().unwrap()
    }

    #[test]
    fn changelog_header_is_parsed() {
        let changelog = "\
# Changelog

My custom changelog header

## [Unreleased]
";
        let header = parse_header(changelog).unwrap();
        let expected_header = "\
# Changelog

My custom changelog header

## [Unreleased]
";
        assert_eq!(header, expected_header);
    }

    #[test]
    fn changelog_header_with_crlf_parsed_will_contain_crlf() {
        let changelog = "# Changelog\r\n\r\nMy custom changelog header\r\n\r\n## [Unreleased]\r\n";
        let header = parse_header(changelog).unwrap_or("".to_string());
        assert_eq!(header, changelog);
    }

    #[test]
    fn changelog_header_without_unreleased_is_parsed() {
        let changelog = "\
# Changelog

My custom changelog header

## [0.2.5] - 2022-12-16

";
        let header = parse_header(changelog).unwrap();
        let expected_header = "\
# Changelog

My custom changelog header
";
        assert_eq!(header, expected_header);
    }

    #[test]
    fn changelog_header_without_unreleased_and_two_previous_versions_is_parsed() {
        let changelog = "\
# Changelog

My custom changelog header

## [0.2.5] - 2022-12-16

### Added

- Incredible feature

## [0.2.5] - 2022-12-16

### Fixed

- Incredible bug
";
        let header = parse_header(changelog).unwrap();
        let expected_header = "\
# Changelog

My custom changelog header
";
        assert_eq!(header, expected_header);
    }

    #[test]
    fn changelog_header_with_versions_is_parsed() {
        let changelog = "\
# Changelog

My custom changelog header

## [Unreleased]

## [0.2.5] - 2022-12-16
";
        let header = parse_header(changelog).unwrap();
        let expected_header = "\
# Changelog

My custom changelog header

## [Unreleased]
";
        assert_eq!(header, expected_header);
    }

    #[test]
    fn changelog_header_isnt_recognized() {
        // A two-level header similar to `## [Unreleased]` is missing
        let changelog = "\
# Changelog

My custom changelog header
";
        let header = parse_header(changelog);
        assert_eq!(header, None);
    }

    #[test]
    fn changelog_with_unreleased_section_is_parsed() {
        let changelog = "\
# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.5] - 2022-12-16

### Added

- Add function to retrieve default branch (#372)

## [0.2.4] - 2022-12-12

### Changed

- improved error message
";
        let changes = last_changes_from_str_test(changelog);
        let expected_changes = "\
### Added

- Add function to retrieve default branch (#372)";
        assert_eq!(changes, expected_changes);
    }

    #[test]
    fn changelog_without_unreleased_section_is_parsed() {
        let changelog = "\
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.5](https://github.com/release-plz/release-plz/compare/git_cmd-v0.2.4...git_cmd-v0.2.5) - 2022-12-16

### Added

- Add function to retrieve default branch (#372)

## [0.2.4] - 2022-12-12

### Changed

- improved error message
";
        let changes = last_changes_from_str_test(changelog);
        let expected_changes = "\
### Added

- Add function to retrieve default branch (#372)";
        assert_eq!(changes, expected_changes);
    }
}
