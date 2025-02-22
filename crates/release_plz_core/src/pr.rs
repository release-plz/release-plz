use crate::{
    PackagesUpdate, ReleaseInfo,
    tera::{PACKAGE_VAR, RELEASES_VAR, VERSION_VAR, render_template},
};
use chrono::SecondsFormat;

pub const DEFAULT_BRANCH_PREFIX: &str = "release-plz-";
pub const OLD_BRANCH_PREFIX: &str = "release-plz/";
pub const DEFAULT_PR_BODY_TEMPLATE: &str = r#"
{% macro get_changes(releases, type="text") %}
{%- for release in releases %}
{%- if release.title and release.changelog %}{% if releases | length > 1 %}
## `{{ release.package }}`
{% endif %}
<blockquote>

## {{ release.title }}

{{ release.changelog }}
</blockquote>{% endif %}
{% endfor %}
{% endmacro -%}

{% set changes = self::get_changes(releases=releases) %}

## 🤖 New release
{% for release in releases %}
* `{{ release.package }}`: {% if release.previous_version and release.previous_version != release.next_version %}{{ release.previous_version }} -> {% endif %}{{ release.next_version }}{% if release.semver_check == "incompatible" %} (⚠ API breaking changes){% elif release.semver_check == "compatible" %} (✓ API compatible changes){% endif %}
{%- endfor %}
{%- for release in releases %}{% if release.breaking_changes %}

### ⚠ `{{ release.package }}` breaking changes

```text
{{ release.breaking_changes }}
```{% endif %}{% endfor %}
{% if changes %}
<details><summary><i><b>Changelog</b></i></summary><p>
{{ changes }}
</p></details>
{% endif %}
---
This PR was generated with [release-plz](https://github.com/release-plz/release-plz/)."#;

#[derive(Debug)]
pub struct Pr {
    pub base_branch: String,
    pub branch: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
    pub labels: Vec<String>,
}

impl Pr {
    pub fn new(
        default_branch: &str,
        packages_to_update: &PackagesUpdate,
        project_contains_multiple_pub_packages: bool,
        branch_prefix: &str,
        title_template: Option<String>,
        body_template: Option<&str>,
    ) -> Self {
        Self {
            branch: release_branch(branch_prefix),
            base_branch: default_branch.to_string(),
            title: pr_title(
                packages_to_update,
                project_contains_multiple_pub_packages,
                title_template,
            ),
            body: pr_body(packages_to_update, body_template),
            draft: false,
            labels: vec![],
        }
    }

    pub fn mark_as_draft(mut self, draft: bool) -> Self {
        self.draft = draft;
        self
    }

    pub fn with_labels(mut self, labels: Vec<String>) -> Self {
        self.labels = labels;
        self
    }
}

fn release_branch(prefix: &str) -> String {
    let now = chrono::offset::Utc::now();
    // Convert to a string of format "2018-01-26T18:30:09Z".
    let now = now.to_rfc3339_opts(SecondsFormat::Secs, true);
    // ':' is not a valid character for a branch name.
    let now = now.replace(':', "-");
    format!("{prefix}{now}")
}

fn pr_title(
    packages_to_update: &PackagesUpdate,
    project_contains_multiple_pub_packages: bool,
    title_template: Option<String>,
) -> String {
    let updates = packages_to_update.updates();
    let first_version = &updates[0].1.version;

    let are_all_versions_equal = || {
        updates
            .iter()
            .all(|(_, update)| &update.version == first_version)
    };

    if let Some(title_template) = title_template {
        let mut context = tera::Context::new();

        if updates.len() == 1 {
            let (package, _) = &updates[0];
            context.insert(PACKAGE_VAR, &package.name);
        }

        if are_all_versions_equal() {
            context.insert(VERSION_VAR, first_version.to_string().as_str());
        }

        render_template(&title_template, &context, "pr_name")
    } else if updates.len() == 1 && project_contains_multiple_pub_packages {
        let (package, _) = &updates[0];
        // The project is a workspace with multiple public packages and we are only updating one of them.
        // Specify which package is being updated in the PR title.
        format!("chore({}): release v{}", package.name, first_version)
    } else if updates.len() > 1 && !are_all_versions_equal() {
        // We are updating multiple packages with different versions, so we don't specify the version in the PR title.
        "chore: release".to_string()
    } else {
        // We are updating either:
        // - a single package without other public packages
        // - multiple packages with the same version.
        // In both cases, we can specify the version in the PR title.
        format!("chore: release v{first_version}")
    }
}

/// The Github API allows a max of 65536 characters in the body field when trying to create a new PR
const MAX_BODY_LEN: usize = 65536;

fn pr_body(packages_to_update: &PackagesUpdate, body_template: Option<&str>) -> String {
    let body_template = body_template.unwrap_or(DEFAULT_PR_BODY_TEMPLATE);

    let mut releases = packages_to_update.releases();
    let first_render = render_pr_body(&releases, body_template);

    if first_render.chars().count() > MAX_BODY_LEN {
        tracing::info!(
            "PR body is longer than {MAX_BODY_LEN} characters. Omitting full changelog."
        );

        releases.iter_mut().for_each(|release| {
            release.changelog = None;
            release.title = None;
        });

        render_pr_body(&releases, body_template)
    } else {
        first_render
    }
}

fn render_pr_body(releases: &[ReleaseInfo], body_template: &str) -> String {
    let mut context = tera::Context::new();
    context.insert(RELEASES_VAR, releases);

    let rendered_body = render_template(body_template, &context, "pr_body");
    trim_pr_body(rendered_body)
}

fn trim_pr_body(body: String) -> String {
    // Make extra sure the body is short enough.
    // If it's not, give up trying to fail gracefully by truncating it to the nearest valid UTF-8 boundary.
    // A grapheme cluster may be cut in half in the process.

    if body.chars().count() > MAX_BODY_LEN {
        tracing::warn!("PR body is still longer than {MAX_BODY_LEN} characters. Truncating as is.");
        body.chars().take(MAX_BODY_LEN).collect()
    } else {
        body
    }
}
