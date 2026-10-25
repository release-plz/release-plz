use regex::Regex;

pub fn replace_unreleased(header: &str, unreleased_link: &str) -> String {
    let re = Regex::new(r"#\s*\[Unreleased\](\(.*\))?").unwrap();
    let result = re.replace(header, &format!("# [Unreleased]({unreleased_link})"));
    result.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreleased_link_is_inserted_if_markdown_link() {
        let header = "# Changelog\n## [Unreleased]";
        let new_header = replace_unreleased(
            header,
            "https://github.com/cargo-generate/cargo-generate/compare/v0.1.2...HEAD",
        );
        expect_test::expect![[r#"
            # Changelog
            ## [Unreleased](https://github.com/cargo-generate/cargo-generate/compare/v0.1.2...HEAD)"#]]
        .assert_eq(&new_header);
    }

    #[test]
    fn unreleased_link_is_replaced() {
        let header = "# Changelog\n## [Unreleased](old)";
        let new_header = replace_unreleased(
            header,
            "https://github.com/cargo-generate/cargo-generate/compare/v0.1.2...HEAD",
        );
        expect_test::expect![[r#"
            # Changelog
            ## [Unreleased](https://github.com/cargo-generate/cargo-generate/compare/v0.1.2...HEAD)"#]]
        .assert_eq(&new_header);
    }

    #[test]
    fn unreleased_link_isnt_inserted_if_not_markdown_link() {
        let header = "# Changelog\n## Unreleased";
        let new_header = replace_unreleased(
            header,
            "https://github.com/cargo-generate/cargo-generate/compare/v0.1.2...HEAD",
        );
        expect_test::expect![[r#"
            # Changelog
            ## Unreleased"#]]
        .assert_eq(&new_header);
    }
}
