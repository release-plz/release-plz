use git_cmd::Repo;
use regex::Regex;

/// Returns the latest tag of the repository in the form of `vX.X.X` or `{package_name}-vX.X.X`
/// * `None` if there are no version tags found matching the regex,
/// * `Tag` if at least one tag is matching the regex
pub fn get_the_latest_repo_tag(repo: &Repo) -> Option<String> {
    /*
    Regex explanation:
    - \b asserts a word boundary to ensure the match is not part of a longer word.
    - ([a-zA-Z0-9_-]+-)? optionally matches a package name consisting of alphanumeric characters, underscores, or hyphens followed by a hyphen. The ? makes this group optional.
    - v matches the letter 'v'.
    - (\d+\.\d+\.\d+) matches the version number in x.x.x format, where \d+ matches one or more digits and \. matches a literal period.
    - \b asserts another word boundary to ensure the match is not part of a longer string.

    Examples of matching formats:
    - v1.2.3
    - v0.2.3
    - tokio-v1.2.3
    - parser-v0.1.2
    */
    let regex = Regex::new(r"\b([a-zA-Z0-9_-]+-)?(v\d+\.\d+\.\d+)\b").unwrap();

    let Some(tags) = repo.get_tags_version_sorted(true) else {
        return None;
    };

    let matching_tags = tags
        .into_iter()
        .filter(|tag| regex.is_match(tag))
        .collect::<Vec<_>>();

    if matching_tags.is_empty() {
        None
    } else {
        Some(
            matching_tags
                .first()
                .expect("we ensured there is at least one matching tag")
                .to_owned(),
        )
    }
}
