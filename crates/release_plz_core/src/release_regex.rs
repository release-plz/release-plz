use crate::tera::{render_template, tera_context};
use anyhow::Context as _;
use regex::Regex;

/// Build a regex from a Tera template for matching release tags.
/// The template supports `{{ package }}` and `{{ version }}` variables.
/// - `{{ package }}` is replaced with the escaped package name
/// - `{{ version }}` is replaced with a semver 2.0 capture group
///
/// ## Why not use `LazyLock`?
///
/// The regex crate docs recommend using `LazyLock` to avoid recompiling the same regex
/// in a loop. However, that only applies to **static** patterns. Here, the pattern is
/// **dynamic** (depends on `template` and `package_name`), so caching in a static isn't
/// applicable. Also, This function is called once per package, not in a hot loop.
///
/// ## Why use Tera instead of simple string replacement?
///
/// We reuse the existing Tera infrastructure to keep template handling consolidated.
/// This ensures the same template syntax works everywhere in release-plz.
pub(crate) fn get_release_regex(template: &str, package_name: &str) -> anyhow::Result<Regex> {
    // Define a unique placeholder so it survives Tera rendering
    // and can be reliably located afterward to find and replace with the regex capture group.
    const VERSION_PLACEHOLDER: &str = "0.0.0-VERSION-PLACEHOLDER";

    // Render the Tera template with the actual package name and our placeholder.
    // For example, template "{{ package }}-v{{ version }}" with package "mylib"
    // renders to "mylib-v0.0.0-VERSION-PLACEHOLDER".
    let context = tera_context(package_name, VERSION_PLACEHOLDER);
    let rendered = render_template(template, &context, "release_tag_name")
        .context("failed to render release tag name template")?;

    // Escape the rendered string for use in a regex.
    // We do this because the template might contain regex metacharacters
    // like `.` (e.g., template "release.{{ version }}").
    let escaped = regex::escape(&rendered);

    // Replace the escaped placeholder with a semver 2.0 capture group.
    // We must escape the placeholder too since `regex::escape` was applied to the whole string.
    // https://semver.org/#is-there-a-suggested-regular-expression-regex-to-check-a-semver-string
    const SEMVER_REGEX: &str = concat!(
        r"(",
        r"(?:0|[1-9]\d*)", // major
        r"\.",
        r"(?:0|[1-9]\d*)", // minor
        r"\.",
        r"(?:0|[1-9]\d*)", // patch
        r"(?:-",           // pre-release (optional)
        r"(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)",
        r"(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*",
        r")?",
        r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?", // build metadata (optional)
        r")",
    );
    let pattern = escaped.replace(&regex::escape(VERSION_PLACEHOLDER), SEMVER_REGEX);

    // Anchor the pattern with ^ and $ to ensure we match the entire tag string,
    // not just a substring. This prevents false matches like "prefix-mylib-v1.2.3-suffix".
    let full_regex = format!(r"^{pattern}$");
    Regex::new(&full_regex).context("build release tag regex")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_regex_version_only_template() {
        let regex = get_release_regex("v{{ version }}", "ignored").unwrap();

        // Matches valid tags
        assert!(regex.is_match("v1.2.3"));
        assert!(regex.is_match("v0.0.1"));
        assert!(regex.is_match("v1.2.3-rc.1"));
        assert!(regex.is_match("v0.1.0-beta"));

        // Rejects invalid formats
        assert!(!regex.is_match("1.2.3")); // missing v
        assert!(!regex.is_match("v1.2")); // incomplete semver
        assert!(!regex.is_match("v1.2.3.4")); // too many parts
        assert!(!regex.is_match("v1.2.3-")); // invalid pre-release

        // Captures version correctly
        let captures = regex.captures("v1.2.3").unwrap();
        assert_eq!(captures.get(1).unwrap().as_str(), "1.2.3");
        let captures = regex.captures("v1.2.3-rc.1").unwrap();
        assert_eq!(captures.get(1).unwrap().as_str(), "1.2.3-rc.1");
    }

    #[test]
    fn release_regex_package_and_version_template() {
        let regex = get_release_regex("{{ package }}-v{{ version }}", "mylib").unwrap();

        // Matches correct package
        assert!(regex.is_match("mylib-v1.2.3"));
        assert!(regex.is_match("mylib-v0.9.0-beta.2"));

        // Rejects wrong package or format
        assert!(!regex.is_match("otherlib-v1.2.3"));
        assert!(!regex.is_match("mylib-1.2.3")); // missing v
        assert!(!regex.is_match("v1.2.3")); // missing package

        // Captures version correctly
        let captures = regex.captures("mylib-v4.5.6").unwrap();
        assert_eq!(captures.get(1).unwrap().as_str(), "4.5.6");
    }

    #[test]
    fn release_regex_custom_template() {
        let regex = get_release_regex("release-{{ version }}-prod", "ignored").unwrap();

        // Matches custom format
        assert!(regex.is_match("release-1.2.3-prod"));
        assert!(regex.is_match("release-1.2.3-rc.1-prod"));

        // Rejects partial matches
        assert!(!regex.is_match("release-1.2.3"));
        assert!(!regex.is_match("v1.2.3"));

        // Captures version correctly
        let captures = regex.captures("release-0.1.0-prod").unwrap();
        assert_eq!(captures.get(1).unwrap().as_str(), "0.1.0");
        let captures = regex.captures("release-1.2.3-rc.1-prod").unwrap();
        assert_eq!(captures.get(1).unwrap().as_str(), "1.2.3-rc.1");
    }

    #[test]
    fn release_regex_escapes_special_chars_in_template() {
        // Template contains `.` which is a regex metacharacter
        let regex = get_release_regex("release.{{ version }}", "ignored").unwrap();

        // Dot is literal, not "any char"
        assert!(regex.is_match("release.1.2.3"));
        assert!(!regex.is_match("releaseX1.2.3"));
    }

    // Registries different from crates.io may allow package names with special characters.
    #[test]
    fn release_regex_escapes_special_chars_in_package_name() {
        let regex = get_release_regex("{{ package }}-v{{ version }}", "my.package").unwrap();

        // Dot is literal, not "any char"
        assert!(regex.is_match("my.package-v1.2.3"));
        assert!(!regex.is_match("myXpackage-v1.2.3"));
    }

    #[test]
    fn release_regex_semver_build_metadata() {
        let regex = get_release_regex("v{{ version }}", "ignored").unwrap();
        assert!(regex.is_match("v1.2.3+build"));
        assert!(regex.is_match("v1.2.3+build.123"));
        assert!(regex.is_match("v1.2.3-rc.1+build.123"));

        let captures = regex.captures("v1.2.3+build.123").unwrap();
        assert_eq!(captures.get(1).unwrap().as_str(), "1.2.3+build.123");
    }

    #[test]
    fn release_regex_strict_semver() {
        let regex = get_release_regex("v{{ version }}", "ignored").unwrap();

        // Leading zeros not allowed
        assert!(!regex.is_match("v01.2.3"));
        assert!(!regex.is_match("v1.02.3"));
        assert!(!regex.is_match("v1.2.03"));

        // Leading zeros in numeric prerelease not allowed
        assert!(!regex.is_match("v1.2.3-01"));
        assert!(regex.is_match("v1.2.3-01a")); // Allowed (alphanumeric)
    }

    #[test]
    fn release_regex_invalid_tera_syntax() {
        let result = get_release_regex("{{ invalid syntax", "mylib");
        assert!(result.is_err());
    }
}
