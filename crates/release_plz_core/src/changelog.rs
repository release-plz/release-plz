use anyhow::Context;
use chrono::{NaiveDate, TimeZone, Utc};
use git_cliff_core::{
    changelog::Changelog as GitCliffChangelog,
    commit::Commit,
    config::{Bump, ChangelogConfig, CommitParser, Config, GitConfig, RemoteConfig, TextProcessor},
    contributor::RemoteContributor,
    release::Release,
};
use regex::Regex;
use serde::Serialize;
use tracing::warn;

use crate::changelog_parser;

pub const CHANGELOG_HEADER: &str = r#"# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
"#;

pub const CHANGELOG_FILENAME: &str = "CHANGELOG.md";
pub const RELEASE_LINK: &str = "release_link";
pub const REMOTE: &str = "remote";

#[derive(Debug)]
pub struct Changelog<'a> {
    release: Release<'a>,
    config: Option<Config>,
    release_link: Option<String>,
    package: String,
    remote: Option<Remote>,
    pr_link: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct Remote {
    /// Owner of the repo. E.g. `MarcoIeni`.
    pub owner: String,
    /// Name of the repo. E.g. `release-plz`.
    pub repo: String,
    /// HTTP URL to the repository.
    /// E.g. `https://github.com/release-plz/release-plz`.
    pub link: String,
    /// List of contributors.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub contributors: Vec<RemoteContributor>,
}

impl Changelog<'_> {
    /// Generate the full changelog.
    pub fn generate(self) -> anyhow::Result<String> {
        let config = self.changelog_config(None);
        let changelog = self.get_changelog(&config)?;
        let mut out = Vec::new();
        changelog
            .generate(&mut out)
            .context("cannot generate changelog")?;
        String::from_utf8(out).context("cannot convert bytes to string")
    }

    /// Update an existing changelog.
    pub fn prepend(self, old_changelog: impl Into<String>) -> anyhow::Result<String> {
        let old_changelog: String = old_changelog.into();
        if is_version_unchanged(&self.release) {
            // The changelog already contains this version, so we don't update the changelog.
            return Ok(old_changelog);
        }
        let old_header = changelog_parser::parse_header(&old_changelog);
        let config = self.changelog_config(old_header.clone());
        let changelog = self.get_changelog(&config)?;

        // If we successfully parsed an old header, compose manually to preserve exact formatting
        // and avoid potential header duplication.
        if let Some(header) = old_header {
            return compose_changelog(&old_changelog, &changelog, header);
        }

        // Fallback: let git-cliff handle the prepend.
        let mut out = Vec::new();
        changelog
            .prepend(old_changelog, &mut out)
            .context("cannot update changelog")?;
        String::from_utf8(out).context("cannot convert bytes to string")
    }

    fn get_changelog<'a>(
        &'a self,
        config: &'a Config,
    ) -> Result<GitCliffChangelog<'a>, anyhow::Error> {
        let mut changelog = GitCliffChangelog::new(vec![self.release.clone()], config, None)
            .context("error while building changelog")?;
        add_package_context(&mut changelog, &self.package)?;
        add_release_link_context(&mut changelog, self.release_link.as_deref())?;
        add_remote_context(&mut changelog, self.remote.as_ref())?;
        Ok(changelog)
    }

    fn changelog_config(&self, header: Option<String>) -> Config {
        let user_config = self.config.clone().unwrap_or(default_git_cliff_config());
        Config {
            changelog: apply_defaults_to_changelog_config(user_config.changelog, header),
            git: apply_defaults_to_git_config(user_config.git, self.pr_link.as_deref()),
            remote: user_config.remote,
            bump: Bump::default(),
        }
    }
}

fn compose_changelog(
    old_changelog: &String,
    changelog: &GitCliffChangelog<'_>,
    header: String,
) -> Result<String, anyhow::Error> {
    let mut new_out = Vec::new();
    changelog
        .generate(&mut new_out)
        .context("cannot generate updated changelog")?;
    let generated = String::from_utf8(new_out).context("cannot convert bytes to string")?;
    let generated_header = crate::changelog_parser::parse_header(&generated);
    let header_to_strip = if let Some(gen_h) = generated_header {
        gen_h
    } else {
        header.clone()
    };
    let generated_body = generated
        .strip_prefix(&header_to_strip)
        .unwrap_or_else(|| generated.as_str());
    let old_body = old_changelog
        .strip_prefix(&header)
        .unwrap_or_else(|| old_changelog.as_str());
    Ok(format!("{header}{generated_body}{old_body}"))
}

/// Apply release-plz defaults to git config
fn apply_defaults_to_git_config(git_config: GitConfig, pr_link: Option<&str>) -> GitConfig {
    let default_git_config = default_git_config(pr_link);

    GitConfig {
        conventional_commits: git_config.conventional_commits,
        require_conventional: git_config.require_conventional,
        filter_unconventional: git_config.filter_unconventional,
        split_commits: git_config.split_commits,
        commit_preprocessors: if git_config.commit_preprocessors.is_empty() {
            default_git_config.commit_preprocessors
        } else {
            git_config.commit_preprocessors
        },
        commit_parsers: if git_config.commit_parsers.is_empty() {
            default_git_config.commit_parsers
        } else {
            git_config.commit_parsers
        },
        protect_breaking_commits: git_config.protect_breaking_commits,
        filter_commits: git_config.filter_commits,
        tag_pattern: git_config.tag_pattern,
        skip_tags: git_config.skip_tags,
        ignore_tags: git_config.ignore_tags,
        count_tags: git_config.count_tags,
        use_branch_tags: git_config.use_branch_tags,
        topo_order: git_config.topo_order,
        topo_order_commits: git_config.topo_order_commits,
        sort_commits: if git_config.sort_commits.is_empty() {
            default_git_config.sort_commits
        } else {
            git_config.sort_commits
        },
        limit_commits: git_config.limit_commits,
        recurse_submodules: git_config.recurse_submodules,
        link_parsers: if git_config.link_parsers.is_empty() {
            default_git_config.link_parsers
        } else {
            git_config.link_parsers
        },
        exclude_paths: git_config.exclude_paths,
        include_paths: git_config.include_paths,
    }
}

fn add_package_context(
    changelog: &mut GitCliffChangelog,
    package: &str,
) -> Result<(), anyhow::Error> {
    changelog
        .add_context("package", package)
        .with_context(|| format!("failed to add `{package}` to the `package` changelog context"))?;
    Ok(())
}

fn add_release_link_context(
    changelog: &mut GitCliffChangelog,
    release_link: Option<&str>,
) -> Result<(), anyhow::Error> {
    if let Some(release_link) = release_link {
        changelog
            .add_context(RELEASE_LINK, release_link)
            .with_context(|| {
                format!(
                    "failed to add `{release_link:?}` to the `{RELEASE_LINK}` changelog context"
                )
            })?;
    }
    Ok(())
}

fn add_remote_context(
    changelog: &mut GitCliffChangelog,
    remote: Option<&Remote>,
) -> Result<(), anyhow::Error> {
    if let Some(remote) = remote {
        add_context(changelog, REMOTE, remote)?;
    }
    Ok(())
}

fn add_context(
    changelog: &mut GitCliffChangelog,
    key: &str,
    value: impl serde::Serialize,
) -> Result<(), anyhow::Error> {
    let value_str = serde_json::to_string(&value).context("failed to serialize value")?;
    changelog
        .add_context(key, value)
        .with_context(|| format!("failed to add `{value_str}` to the `{key}` changelog context"))
}

/// Apply release-plz defaults
fn apply_defaults_to_changelog_config(
    changelog: ChangelogConfig,
    header: Option<String>,
) -> ChangelogConfig {
    let default_changelog_config = default_changelog_config(header);

    ChangelogConfig {
        header: changelog.header.or(default_changelog_config.header),
        body: if changelog.body.is_empty() {
            default_changelog_config.body
        } else {
            changelog.body
        },
        footer: changelog.footer.or(default_changelog_config.footer),
        trim: changelog.trim,
        render_always: changelog.render_always,
        postprocessors: if changelog.postprocessors.is_empty() {
            default_changelog_config.postprocessors
        } else {
            changelog.postprocessors
        },
        output: changelog.output.or(default_changelog_config.output),
    }
}

fn is_version_unchanged(release: &Release) -> bool {
    let previous_version = release.previous.as_ref().and_then(|r| r.version.as_deref());
    let new_version = release.version.as_deref();
    previous_version == new_version
}

fn default_git_cliff_config() -> Config {
    Config {
        changelog: default_changelog_config(None),
        git: default_git_config(None),
        remote: RemoteConfig::default(),
        bump: Bump::default(),
    }
}

#[derive(Debug, Clone)]
pub struct ChangelogBuilder<'a> {
    commits: Vec<Commit<'a>>,
    version: String,
    previous_version: Option<String>,
    config: Option<Config>,
    remote: Option<Remote>,
    release_date: Option<NaiveDate>,
    release_link: Option<String>,
    package: String,
    pr_link: Option<String>,
}

impl<'a> ChangelogBuilder<'a> {
    pub fn new(
        commits: Vec<Commit<'a>>,
        version: impl Into<String>,
        package: impl Into<String>,
    ) -> Self {
        Self {
            commits,
            version: version.into(),
            previous_version: None,
            config: None,
            release_date: None,
            remote: None,
            release_link: None,
            package: package.into(),
            pr_link: None,
        }
    }

    pub fn with_previous_version(self, previous_version: impl Into<String>) -> Self {
        Self {
            previous_version: Some(previous_version.into()),
            ..self
        }
    }

    pub fn with_pr_link(self, pr_link: impl Into<String>) -> Self {
        Self {
            pr_link: Some(pr_link.into()),
            ..self
        }
    }

    pub fn with_release_date(self, release_date: NaiveDate) -> Self {
        Self {
            release_date: Some(release_date),
            ..self
        }
    }

    pub fn with_release_link(self, release_link: impl Into<String>) -> Self {
        Self {
            release_link: Some(release_link.into()),
            ..self
        }
    }

    pub fn with_config(self, config: Config) -> Self {
        Self {
            config: Some(config),
            ..self
        }
    }

    pub fn with_remote(self, remote: Remote) -> Self {
        Self {
            remote: Some(remote),
            ..self
        }
    }

    pub fn config(&self) -> Option<&Config> {
        self.config.as_ref()
    }

    pub fn build(&self) -> Changelog<'a> {
        let git_config = self
            .config
            .clone()
            .map(|c| c.git)
            .unwrap_or_else(|| default_git_config(self.pr_link.as_deref()));
        let release_date = self.release_timestamp();
        let mut commits: Vec<_> = self
            .commits
            .iter()
            .filter_map(|c| c.process(&git_config).ok())
            .collect();

        match git_config.sort_commits.to_lowercase().as_str() {
            "oldest" => {
                commits.reverse();
            }
            "newest" => {
                // commits are already sorted from newest to oldest, we don't need to do anything
            }
            other => {
                warn!(
                    "Invalid setting for sort_commits: '{other}'. Valid values are 'newest' and 'oldest'."
                );
            }
        }

        let previous = self.previous_version.as_ref().map(|ver| Release {
            version: Some(ver.clone()),
            commits: vec![],
            commit_id: None,
            timestamp: Some(0),
            previous: None,
            message: None,
            repository: None,
            ..Default::default()
        });

        Changelog {
            release: Release {
                version: Some(self.version.clone()),
                commits,
                commit_id: None,
                timestamp: Some(release_date),
                previous: previous.map(Box::new),
                message: None,
                repository: None,
                ..Default::default()
            },
            remote: self.remote.clone(),
            release_link: self.release_link.clone(),
            config: self.config.clone(),
            package: self.package.clone(),
            pr_link: self.pr_link.clone(),
        }
    }

    /// Returns the provided release timestamp, if provided.
    /// Current timestamp otherwise.
    fn release_timestamp(&self) -> i64 {
        self.release_date
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|d| Utc.from_utc_datetime(&d))
            .unwrap_or_else(Utc::now)
            .timestamp()
    }
}

pub fn default_git_config(pr_link: Option<&str>) -> GitConfig {
    GitConfig {
        conventional_commits: true,
        filter_unconventional: false,
        commit_parsers: kac_commit_parsers(),
        filter_commits: false,
        tag_pattern: None,
        skip_tags: None,
        split_commits: false,
        protect_breaking_commits: false,
        topo_order: false,
        ignore_tags: None,
        limit_commits: None,
        sort_commits: "newest".to_string(),
        commit_preprocessors: pr_link
            .map(|pr_link| {
                // Replace #123 with [#123](https://link_to_pr).
                // If the number refers to an issue, GitHub redirects the PR link to the issue link.
                vec![TextProcessor {
                    pattern: Regex::new(r"\(#([0-9]+)\)").expect("invalid regex"),
                    replace: Some(format!("([#${{1}}]({pr_link}/${{1}}))")),
                    replace_command: None,
                }]
            })
            .unwrap_or_default(),
        link_parsers: vec![],
        ..Default::default()
    }
}

fn commit_parser(regex: &str, group: &str) -> CommitParser {
    CommitParser {
        message: Regex::new(regex).ok(),
        body: None,
        group: Some(group.to_string()),
        default_scope: None,
        scope: None,
        skip: None,
        field: None,
        pattern: None,
        sha: None,
        footer: None,
    }
}

/// Commit parsers based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
fn kac_commit_parsers() -> Vec<CommitParser> {
    vec![
        commit_parser("^feat", "added"),
        commit_parser("^changed", "changed"),
        commit_parser("^deprecated", "deprecated"),
        commit_parser("^removed", "removed"),
        commit_parser("^fix", "fixed"),
        commit_parser("^security", "security"),
        commit_parser(".*", "other"),
    ]
}

pub fn default_changelog_config(header: Option<String>) -> ChangelogConfig {
    ChangelogConfig {
        header: Some(header.unwrap_or(String::from(CHANGELOG_HEADER))),
        body: default_changelog_body_config().to_string(),
        footer: None,
        postprocessors: vec![],
        trim: true,
        ..ChangelogConfig::default()
    }
}

fn default_changelog_body_config() -> &'static str {
    r#"
## [{{ version }}]{%- if release_link -%}({{ release_link }}){% endif %} - {{ timestamp | date(format="%Y-%m-%d") }}
{% for group, commits in commits | group_by(attribute="group") %}
### {{ group | upper_first }}

{% for commit in commits %}
{%- if commit.scope -%}
- *({{commit.scope}})* {% if commit.breaking %}[**breaking**] {% endif %}{{ commit.message }}{%- if commit.links %} ({% for link in commit.links %}[{{link.text}}]({{link.href}}) {% endfor -%}){% endif %}
{% else -%}
- {% if commit.breaking %}[**breaking**] {% endif %}{{ commit.message }}
{% endif -%}
{% endfor -%}
{% endfor %}"#
}

#[cfg(test)]
mod tests {
    use crate::NO_COMMIT_ID;

    use super::*;

    #[test]
    fn changelog_entries_are_generated() {
        let commits = vec![
            Commit::new(NO_COMMIT_ID.to_string(), "fix: myfix".to_string()),
            Commit::new(NO_COMMIT_ID.to_string(), "simple update".to_string()),
        ];
        let changelog = ChangelogBuilder::new(commits, "1.1.1", "my_pkg")
            .with_release_date(NaiveDate::from_ymd_opt(2015, 5, 15).unwrap())
            .build();

        expect_test::expect![[r#"
            # Changelog

            All notable changes to this project will be documented in this file.

            The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
            and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

            ## [Unreleased]

            ## [1.1.1] - 2015-05-15

            ### Fixed

            - myfix

            ### Other

            - simple update
        "#]]
        .assert_eq(&changelog.generate().unwrap());
    }

    #[test]
    fn changelog_entry_with_link_is_generated() {
        let commits = vec![Commit::new(
            NO_COMMIT_ID.to_string(),
            "fix: myfix".to_string(),
        )];
        let changelog = ChangelogBuilder::new(commits, "1.1.1", "my_pkg")
            .with_release_date(NaiveDate::from_ymd_opt(2015, 5, 15).unwrap())
            .with_release_link("https://github.com/release-plz/release-plz/compare/release-plz-v0.2.24...release-plz-v0.2.25")
            .build();

        expect_test::expect![[r#"
            # Changelog

            All notable changes to this project will be documented in this file.

            The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
            and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

            ## [Unreleased]

            ## [1.1.1](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.24...release-plz-v0.2.25) - 2015-05-15

            ### Fixed

            - myfix
        "#]]
        .assert_eq(&changelog.generate().unwrap());
    }

    #[test]
    fn generated_changelog_is_updated_correctly() {
        let commits = vec![
            Commit::new(NO_COMMIT_ID.to_string(), "fix: myfix".to_string()),
            Commit::new(NO_COMMIT_ID.to_string(), "simple update".to_string()),
        ];
        let changelog = ChangelogBuilder::new(commits, "1.1.1", "my_pkg")
            .with_release_date(NaiveDate::from_ymd_opt(2015, 5, 15).unwrap())
            .build();

        let generated_changelog = changelog.generate().unwrap();

        let commits = vec![
            Commit::new(NO_COMMIT_ID.to_string(), "fix: myfix2".to_string()),
            Commit::new(NO_COMMIT_ID.to_string(), "complex update".to_string()),
        ];
        let changelog = ChangelogBuilder::new(commits, "1.1.2", "my_pkg")
            .with_release_date(NaiveDate::from_ymd_opt(2015, 5, 15).unwrap())
            .build();

        expect_test::expect![[r#"
            # Changelog

            All notable changes to this project will be documented in this file.

            The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
            and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

            ## [Unreleased]

            ## [1.1.2] - 2015-05-15

            ### Fixed

            - myfix2

            ### Other

            - complex update

            ## [1.1.1] - 2015-05-15

            ### Fixed

            - myfix

            ### Other

            - simple update
        "#]]
        .assert_eq(&changelog.prepend(generated_changelog).unwrap());
    }

    #[test]
    fn changelog_is_updated() {
        let commits = vec![
            Commit::new(NO_COMMIT_ID.to_string(), "fix: myfix".to_string()),
            Commit::new(NO_COMMIT_ID.to_string(), "simple update".to_string()),
        ];
        let changelog = ChangelogBuilder::new(commits, "1.1.1", "my_pkg")
            .with_release_date(NaiveDate::from_ymd_opt(2015, 5, 15).unwrap())
            .build();
        let old_body = r#"## [1.1.0] - 1970-01-01

### fix bugs

- my awesomefix

### other

- complex update
"#;
        let old = format!("{CHANGELOG_HEADER}\n{old_body}");
        let new = changelog.prepend(old).unwrap();
        expect_test::expect![[r#"
            # Changelog

            All notable changes to this project will be documented in this file.

            The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
            and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

            ## [Unreleased]

            ## [1.1.1] - 2015-05-15

            ### Fixed

            - myfix

            ### Other

            - simple update

            ## [1.1.0] - 1970-01-01

            ### fix bugs

            - my awesomefix

            ### other

            - complex update
        "#]]
        .assert_eq(&new);
    }

    #[test]
    fn changelog_without_header_is_updated() {
        let commits = vec![
            Commit::new(NO_COMMIT_ID.to_string(), "fix: myfix".to_string()),
            Commit::new(NO_COMMIT_ID.to_string(), "simple update".to_string()),
        ];
        let changelog = ChangelogBuilder::new(commits, "1.1.1", "my_pkg")
            .with_release_date(NaiveDate::from_ymd_opt(2015, 5, 15).unwrap())
            .build();
        let old = r#"
## [1.1.0] - 1970-01-01

### fix bugs

- my awesomefix

### other

- complex update
"#;
        let new = changelog.prepend(old);
        expect_test::expect![[r#"
            # Changelog

            All notable changes to this project will be documented in this file.

            The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
            and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

            ## [Unreleased]

            ## [1.1.1] - 2015-05-15

            ### Fixed

            - myfix

            ### Other

            - simple update

            ## [1.1.0] - 1970-01-01

            ### fix bugs

            - my awesomefix

            ### other

            - complex update
        "#]]
        .assert_eq(&new.unwrap());
    }

    #[test]
    fn changelog_has_commit_id() {
        let commits = vec![
            Commit::new("1111111".to_string(), "fix: myfix".to_string()),
            Commit::new(
                NO_COMMIT_ID.to_string(),
                "chore: something else".to_string(),
            ),
        ];
        let changelog = ChangelogBuilder::new(commits, "1.1.1", "my_pkg")
            .with_release_date(NaiveDate::from_ymd_opt(2015, 5, 15).unwrap())
            .with_config(Config {
                changelog: ChangelogConfig {
                    header: Some("# Changelog".to_string()),
                    body: r"{%- for commit in commits %}
                            {{ commit.message }} - {{ commit.id }}
                        {% endfor -%}"
                        .to_string(),
                    ..default_changelog_config(None)
                },
                git: default_git_config(None),
                remote: RemoteConfig::default(),
                bump: Bump::default(),
            })
            .build();

        expect_test::expect![[r#"
            # Changelog

            myfix - 1111111

            something else - 0000000
        "#]]
        .assert_eq(&changelog.generate().unwrap());
    }

    #[test]
    fn changelog_sort_newest() {
        let commits = vec![
            Commit::new("1111111".to_string(), "fix: myfix".to_string()),
            Commit::new("0000000".to_string(), "fix: another fix".to_string()),
        ];
        let changelog = ChangelogBuilder::new(commits, "1.1.1", "my_pkg")
            .with_release_date(NaiveDate::from_ymd_opt(2015, 5, 15).unwrap())
            .with_config(Config {
                changelog: default_changelog_config(None),
                git: GitConfig {
                    sort_commits: "oldest".to_string(),
                    ..default_git_config(None)
                },
                remote: RemoteConfig::default(),
                bump: Bump::default(),
            })
            .build();

        expect_test::expect![[r#"
            # Changelog

            All notable changes to this project will be documented in this file.

            The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
            and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

            ## [Unreleased]

            ## [1.1.1] - 2015-05-15

            ### Fixed

            - another fix
            - myfix
        "#]]
        .assert_eq(&changelog.generate().unwrap());
    }
}

#[test]
fn empty_changelog_is_updated() {
    let commits = vec![
        Commit::new(crate::NO_COMMIT_ID.to_string(), "fix: myfix".to_string()),
        Commit::new(crate::NO_COMMIT_ID.to_string(), "simple update".to_string()),
    ];
    let changelog = ChangelogBuilder::new(commits, "1.1.1", "my_pkg")
        .with_release_date(NaiveDate::from_ymd_opt(2015, 5, 15).unwrap())
        .build();
    let new = changelog.prepend(CHANGELOG_HEADER);
    expect_test::expect![[r#"
        # Changelog

        All notable changes to this project will be documented in this file.

        The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
        and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

        ## [Unreleased]

        ## [1.1.1] - 2015-05-15

        ### Fixed

        - myfix

        ### Other

        - simple update
    "#]]
    .assert_eq(&new.unwrap());
}
