use crate::{
    tera::{render_template, PACKAGE_VAR, RELEASES_VAR, VERSION_VAR},
    PackagesUpdate,
};
use chrono::SecondsFormat;

pub const DEFAULT_BRANCH_PREFIX: &str = "release-plz-";
pub const OLD_BRANCH_PREFIX: &str = "release-plz/";

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
        body_template: Option<String>,
    ) -> Self {
        Self {
            branch: pr_branch_name(branch_prefix, packages_to_update),
            base_branch: default_branch.to_string(),
            title: pr_title(
                packages_to_update,
                project_contains_multiple_pub_packages,
                title_template,
            ),
            body: pr_body(
                packages_to_update,
                project_contains_multiple_pub_packages,
                body_template,
            ),
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

/// Generate the prefix for the PR branch name.
fn pr_branch_name(branch_prefix: &str, packages_to_update: &PackagesUpdate) -> String {
    let context = create_tera_context(packages_to_update);
    let pr_branch_prefix = render_template(branch_prefix, &context, "pr_branch_prefix");

    release_branch(&pr_branch_prefix)
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
        let context = create_tera_context(packages_to_update);
        render_template(&title_template, &context, "pr_name")
    } else if updates.len() == 1 && project_contains_multiple_pub_packages {
        let (package, _) = &updates[0];
        // The project is a workspace with multiple public packages and we are only updating one of them.
        // Specify which package is being updated in the PR title.
        format!("chore({}): release v{first_version}", package.name)
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

fn create_tera_context(packages_to_update: &PackagesUpdate) -> tera::Context {
    let updates = packages_to_update.updates();
    let first_version = &updates[0].1.version;

    let are_all_versions_equal = || {
        updates
            .iter()
            .all(|(_, update)| &update.version == first_version)
    };

    let mut context = tera::Context::new();

    if updates.len() == 1 {
        let (package, _) = &updates[0];
        context.insert(PACKAGE_VAR, &package.name);
    }

    if are_all_versions_equal() {
        context.insert(VERSION_VAR, first_version.to_string().as_str());
    }

    context
}

/// The Github API allows a max of 65536 characters in the body field when trying to create a new PR
const MAX_BODY_LEN: usize = 65536;

fn pr_body(
    packages_to_update: &PackagesUpdate,
    project_contains_multiple_pub_packages: bool,
    body_template: Option<String>,
) -> String {
    if let Some(body_template) = body_template {
        pr_body_custom(packages_to_update, body_template.as_str())
    } else {
        pr_body_default(packages_to_update, project_contains_multiple_pub_packages)
    }
}

fn pr_body_custom(packages_to_update: &PackagesUpdate, body_template: &str) -> String {
    let releases = packages_to_update.releases();
    let mut context = tera::Context::new();
    context.insert(RELEASES_VAR, &releases);

    trim_pr_body(render_template(body_template, &context, "pr_body"))
}

fn pr_body_default(
    packages_to_update: &PackagesUpdate,
    project_contains_multiple_pub_packages: bool,
) -> String {
    let header = "## 🤖 New release";

    let summary = packages_to_update.summary();
    let changes = {
        let changes = packages_to_update.changes(project_contains_multiple_pub_packages);
        format!(
            "<details><summary><i><b>Changelog</b></i></summary><p>\n\n{changes}\n</p></details>\n"
        )
    };

    let footer =
        "---\nThis PR was generated with [release-plz](https://github.com/release-plz/release-plz/).";
    let mut formatted = format!("{header}{summary}\n{changes}\n{footer}");

    // Make sure we don't go over the Github API's limit for PR body size
    if formatted.chars().count() > MAX_BODY_LEN {
        tracing::info!(
            "PR body is longer than {MAX_BODY_LEN} characters. Omitting full changelog."
        );
        formatted = format!("{header}{summary}\n{footer}");
    }

    trim_pr_body(formatted)
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
