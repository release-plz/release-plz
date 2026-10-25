use regex::Regex;
use serde::Serialize;
use url::Url;

#[derive(Debug, PartialEq, Serialize)]
pub struct Pr {
    html_url: Url,
    pub number: u64,
}

/// Parse PRs from text, e.g. a changelog entry.
pub fn prs_from_text(text: &str) -> Vec<Pr> {
    // given a text, extract all the PRs
    // each PR is a link ending with `/pull/<number>`, `/pulls/<number>`,
    // or `/-/merge_requests/<number>`.
    let re = Regex::new(r"https?://[^\s]+/(?:pulls?|-/merge_requests)/(\d+)").unwrap();

    re.captures_iter(text)
        .filter_map(|capture| {
            let number = capture.get(1)?.as_str().parse().ok()?;
            let html_url = capture.get(0)?.as_str().to_owned();
            Url::parse(&html_url).ok().map(|url| Pr {
                number,
                html_url: url,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pr_links_for_all_forges() {
        let links = [
            ("https://github.com/owner/repo/pull/1", 1),
            ("https://gitea.example.com/owner/repo/pulls/2", 2),
            ("https://gitlab.com/owner/repo/-/merge_requests/3", 3),
            (
                "https://git.company.com/group/subgroup/repo/-/merge_requests/4",
                4,
            ),
        ];
        let changelog = links
            .iter()
            .map(|(url, number)| format!("- Change ([#{number}]({url}))"))
            .collect::<Vec<_>>()
            .join("\n");
        let expected = links
            .into_iter()
            .map(|(url, number)| Pr {
                number,
                html_url: Url::parse(url).unwrap(),
            })
            .collect::<Vec<_>>();
        assert_eq!(prs_from_text(&changelog), expected);
    }

    #[test]
    fn parse_pr_correctly() {
        let changelog_entry = r"
### Added
- use cargo registry environment variable to authenticate in private sparse registry ([#1435](https://github.com/release-plz/release-plz/pull/1435))

### Other
- add `needless_pass_by_value` lint ([#1441](https://github.com/release-plz/release-plz/pull/1441))
- add `uninlined_format_args` ([#1440](https://github.com/release-plz/release-plz/pull/1440))
- add clippy lints ([#1439](https://github.com/release-plz/release-plz/pull/1439))
- add `if_not_else` clippy lint ([#1438](https://github.com/release-plz/release-plz/pull/1438))
- update dependencies ([#1437](https://github.com/release-plz/release-plz/pull/1437))
";
        let prs = prs_from_text(changelog_entry);
        assert_eq!(
            prs,
            vec![
                Pr {
                    number: 1435,
                    html_url: Url::parse("https://github.com/release-plz/release-plz/pull/1435")
                        .unwrap()
                },
                Pr {
                    number: 1441,
                    html_url: Url::parse("https://github.com/release-plz/release-plz/pull/1441")
                        .unwrap()
                },
                Pr {
                    number: 1440,
                    html_url: Url::parse("https://github.com/release-plz/release-plz/pull/1440")
                        .unwrap()
                },
                Pr {
                    number: 1439,
                    html_url: Url::parse("https://github.com/release-plz/release-plz/pull/1439")
                        .unwrap()
                },
                Pr {
                    number: 1438,
                    html_url: Url::parse("https://github.com/release-plz/release-plz/pull/1438")
                        .unwrap()
                },
                Pr {
                    number: 1437,
                    html_url: Url::parse("https://github.com/release-plz/release-plz/pull/1437")
                        .unwrap()
                },
            ]
        );
    }
}
