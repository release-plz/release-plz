use anyhow::Context;
use git_cliff_core::config::{Bump, ChangelogConfig, RemoteConfig};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangelogCfg {
    /// Text at the beginning of the changelog.
    pub header: Option<String>,
    /// Template that represents a single release in the changelog.
    /// It contains the commit messages.
    /// This is a [tera](https://keats.github.io/tera/) template.
    pub body: Option<String>,
    /// If set to `true`, leading and trailing whitespace are removed from [`Self::body`].
    pub trim: Option<bool>,
    /// An array of commit preprocessors for manipulating the commit messages before parsing/grouping them.
    pub commit_preprocessors: Option<Vec<TextProcessor>>,
    /// How to sort the commits inside the various sections.
    pub sort_commits: Option<Sorting>,
    /// An array of link parsers for extracting external references, and turning them into URLs, using regex.
    pub link_parsers: Option<Vec<LinkParser>>,
    /// Commits that don't match any of the commit parsers are skipped.
    pub commit_parsers: Option<Vec<CommitParser>>,
    /// Whether to protect all breaking changes from being skipped by a commit parser.
    pub protect_breaking_commits: Option<bool>,
    /// A regular expression for matching the git tags to add to the changelog.
    pub tag_pattern: Option<String>,
}

impl ChangelogCfg {
    pub fn is_default(&self) -> bool {
        let default_config = ChangelogCfg::default();
        &default_config == self
    }
}

/// Used for modifying commit messages.
#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Debug, Clone, JsonSchema)]
pub struct TextProcessor {
    /// Regex for matching a text to replace.
    pub pattern: String,
    /// Replacement text.
    pub replace: Option<String>,
    /// Command that will be run for replacing the commit message.
    pub replace_command: Option<String>,
}

impl TryFrom<TextProcessor> for git_cliff_core::config::TextProcessor {
    fn try_from(cfg: TextProcessor) -> Result<Self, Self::Error> {
        Ok(Self {
            pattern: to_regex(&cfg.pattern, "pattern")?,
            replace: cfg.replace,
            replace_command: cfg.replace_command,
        })
    }

    type Error = anyhow::Error;
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Sorting {
    Oldest,
    Newest,
}

impl std::fmt::Display for Sorting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Sorting::Oldest => write!(f, "oldest"),
            Sorting::Newest => write!(f, "newest"),
        }
    }
}

#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Debug, Clone, JsonSchema)]
pub struct LinkParser {
    /// Regex for finding links in the commit message.
    pub pattern: String,
    /// The string used to generate the link URL.
    pub href: String,
    /// The string used to generate the link text.
    pub text: Option<String>,
}

impl TryFrom<LinkParser> for git_cliff_core::config::LinkParser {
    type Error = anyhow::Error;

    fn try_from(value: LinkParser) -> Result<Self, Self::Error> {
        Ok(Self {
            pattern: to_regex(&value.pattern, "pattern")?,
            href: value.href,
            text: value.text,
        })
    }
}

/// Parser for grouping commits.
#[derive(Serialize, Deserialize, Default, PartialEq, Eq, Debug, Clone, JsonSchema)]
pub struct CommitParser {
    /// Regex for matching the commit message.
    pub message: Option<String>,
    /// Regex for matching the commit body.
    pub body: Option<String>,
    /// Group of the commit.
    pub group: Option<String>,
    /// Default scope of the commit.
    pub default_scope: Option<String>,
    /// Commit scope for overriding the default scope.
    pub scope: Option<String>,
    /// Whether to skip this commit group.
    pub skip: Option<bool>,
    /// Field name of the commit to match the regex against.
    pub field: Option<String>,
    /// Regex for matching the field value.
    pub pattern: Option<String>,
    /// SHA1 of the commit.
    pub sha: Option<String>,
}

impl TryFrom<CommitParser> for git_cliff_core::config::CommitParser {
    type Error = anyhow::Error;

    fn try_from(cfg: CommitParser) -> Result<Self, Self::Error> {
        Ok(Self {
            message: to_opt_regex(cfg.message.as_deref(), "message")?,
            body: to_opt_regex(cfg.body.as_deref(), "body")?,
            group: cfg.group,
            default_scope: cfg.default_scope,
            scope: cfg.scope,
            skip: cfg.skip,
            field: cfg.field,
            pattern: to_opt_regex(cfg.pattern.as_deref(), "pattern")?,
            sha: cfg.sha,
            footer: None,
        })
    }
}

fn to_regex(input: &str, element_name: &str) -> anyhow::Result<Regex> {
    Regex::new(input).with_context(|| format!("failed to parse `{element_name}` regex"))
}

/// Convert an input string to an (optional) regex.
fn to_opt_regex(input: Option<&str>, element_name: &str) -> anyhow::Result<Option<Regex>> {
    input.map(|i| to_regex(i, element_name)).transpose()
}

fn to_opt_vec<T, U>(vec: Option<Vec<T>>, element_name: &str) -> anyhow::Result<Vec<U>>
where
    T: TryInto<U, Error = anyhow::Error>,
{
    vec.map(|v| vec_try_into(v, element_name))
        .transpose()
        .map(|v| v.unwrap_or_default())
}

fn vec_try_into<T, U>(vec: Vec<T>, element_name: &str) -> anyhow::Result<Vec<U>>
where
    T: TryInto<U, Error = anyhow::Error>,
{
    vec.into_iter()
        .map(|cp| {
            cp.try_into()
                .with_context(|| format!("failed to parse {element_name}"))
        })
        .collect()
}

pub fn to_git_cliff_config(
    cfg: ChangelogCfg,
    pr_link: Option<&str>,
) -> anyhow::Result<git_cliff_core::config::Config> {
    let commit_preprocessors: Vec<git_cliff_core::config::TextProcessor> =
        to_opt_vec(cfg.commit_preprocessors, "commit_preprocessors")?;
    let link_parsers: Vec<git_cliff_core::config::LinkParser> =
        to_opt_vec(cfg.link_parsers, "link_parsers")?;
    let tag_pattern = to_opt_regex(cfg.tag_pattern.as_deref(), "tag_pattern")?;

    let sort_commits = cfg.sort_commits.map(|s| format!("{s}"));

    let commit_parsers: Vec<git_cliff_core::config::CommitParser> =
        to_opt_vec(cfg.commit_parsers, "commit_parsers")?;

    let default_changelog_config = release_plz_core::default_changelog_config(cfg.header.clone());
    let default_git_config = release_plz_core::default_git_config(pr_link);
    Ok(git_cliff_core::config::Config {
        changelog: ChangelogConfig {
            header: default_changelog_config.header,
            body: cfg.body.unwrap_or(default_changelog_config.body),
            trim: cfg.trim.unwrap_or(default_changelog_config.trim),
            postprocessors: vec![],
            footer: None,
            ..ChangelogConfig::default()
        },
        git: git_cliff_core::config::GitConfig {
            conventional_commits: default_git_config.conventional_commits,
            filter_unconventional: default_git_config.filter_unconventional,
            split_commits: default_git_config.split_commits,
            commit_preprocessors,
            commit_parsers,
            protect_breaking_commits: cfg
                .protect_breaking_commits
                .unwrap_or(default_git_config.protect_breaking_commits),
            link_parsers,
            filter_commits: default_git_config.filter_commits,
            tag_pattern,
            skip_tags: None,
            ignore_tags: None,
            topo_order: default_git_config.topo_order,
            sort_commits: sort_commits.unwrap_or(default_git_config.sort_commits),
            limit_commits: None,
            ..Default::default()
        },
        remote: RemoteConfig::default(),
        bump: Bump::default(),
    })
}

// write test to check that the configuration is deserialized correctly
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn test_deserialize_toml() {
        let toml = r#"
            [changelog]
            header = "Changelog"
            body = "Body"
            trim = true
            protect_breaking_commits = true

            commit_preprocessors = [
                { pattern = "pattern", replace = "replace", replace_command = "replace_command" },
                { pattern = "pattern2", replace = "replace2", replace_command = "replace_command2" }
            ]

            commit_parsers = [
                { message = "message", body = "body", group = "group", default_scope = "default_scope", scope = "scope", skip = true, field = "field", pattern = "pattern"}
            ]

            link_parsers = [
                { pattern = "pattern", href = "href", text = "text" }
            ]
    "#;
        let cfg: Config = toml::from_str(toml).unwrap();
        let actual_cliff_config: git_cliff_core::config::Config =
            to_git_cliff_config(cfg.changelog, None).unwrap();
        let expected_cliff_config = git_cliff_core::config::Config {
            changelog: ChangelogConfig {
                header: Some("Changelog".to_string()),
                body: "Body".to_string(),
                trim: true,
                postprocessors: vec![],
                footer: None,
                ..ChangelogConfig::default()
            },
            git: git_cliff_core::config::GitConfig {
                protect_breaking_commits: true,
                commit_preprocessors: vec![
                    git_cliff_core::config::TextProcessor {
                        pattern: regex::Regex::new("pattern").unwrap(),
                        replace: Some("replace".to_string()),
                        replace_command: Some("replace_command".to_string()),
                    },
                    git_cliff_core::config::TextProcessor {
                        pattern: regex::Regex::new("pattern2").unwrap(),
                        replace: Some("replace2".to_string()),
                        replace_command: Some("replace_command2".to_string()),
                    },
                ],
                commit_parsers: vec![git_cliff_core::config::CommitParser {
                    message: Some(regex::Regex::new("message").unwrap()),
                    body: Some(regex::Regex::new("body").unwrap()),
                    group: Some("group".to_string()),
                    default_scope: Some("default_scope".to_string()),
                    scope: Some("scope".to_string()),
                    skip: Some(true),
                    field: Some("field".to_string()),
                    pattern: Some(regex::Regex::new("pattern").unwrap()),
                    sha: None,
                    footer: None,
                }],
                link_parsers: vec![git_cliff_core::config::LinkParser {
                    pattern: regex::Regex::new("pattern").unwrap(),
                    href: "href".to_string(),
                    text: Some("text".to_string()),
                }],
                filter_commits: true,
                tag_pattern: None,
                skip_tags: None,
                ignore_tags: None,
                topo_order: false,
                sort_commits: "newest".to_string(),
                limit_commits: None,
                conventional_commits: true,
                filter_unconventional: false,
                split_commits: false,
                ..Default::default()
            },
            remote: RemoteConfig::default(),
            bump: Bump::default(),
        };

        let expected_cliff_toml = toml::to_string(&expected_cliff_config).unwrap();
        dbg!(&actual_cliff_config);
        let actual_cliff_toml = toml::to_string(&actual_cliff_config).unwrap();
        assert_eq!(expected_cliff_toml, actual_cliff_toml);
    }
}
