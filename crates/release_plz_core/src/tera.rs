use anyhow::Context as _;

use crate::Remote;

pub const PACKAGE_VAR: &str = "package";
pub const VERSION_VAR: &str = "version";
pub const CHANGELOG_VAR: &str = "changelog";
pub const REMOTE_VAR: &str = "remote";
pub const RELEASES_VAR: &str = "releases";

pub fn tera_var(var_name: &str) -> String {
    format!("{{{{ {var_name} }}}}")
}

/// Returns the default Tera template for git tag names based on project structure.
///
/// - Multi-package workspace: `{{ package }}-v{{ version }}` (e.g., `mylib-v1.2.3`)
/// - Single package: `v{{ version }}` (e.g., `v1.2.3`)
///
/// This is used as the default for both `git_tag_name` and `git_only_release_tag_name`
/// when no custom template is specified.
pub fn default_tag_name_template(is_multi_package: bool) -> String {
    if is_multi_package {
        format!("{}-v{}", tera_var(PACKAGE_VAR), tera_var(VERSION_VAR))
    } else {
        format!("v{}", tera_var(VERSION_VAR))
    }
}

pub fn release_body_from_template(
    package_name: &str,
    version: &str,
    changelog: &str,
    remote: &Remote,
    body_template: Option<&str>,
) -> anyhow::Result<String> {
    let mut context = tera_context(package_name, version);
    context.insert(CHANGELOG_VAR, changelog);
    context.insert(REMOTE_VAR, remote);

    let default_body_template = tera_var(CHANGELOG_VAR);
    let body_template = body_template.unwrap_or(&default_body_template);

    render_template(body_template, &context, "release_body")
}

pub fn render_template(
    template: &str,
    context: &tera::Context,
    template_name: &str,
) -> anyhow::Result<String> {
    let mut tera = tera::Tera::default();

    tera.add_raw_template(template_name, template)
        .context("failed to add release_body raw template")?;

    tera.render(template_name, context)
        .with_context(|| format!("failed to render {template_name}"))
}

pub fn tera_context(package_name: &str, version: &str) -> tera::Context {
    let mut context = tera::Context::new();
    context.insert(PACKAGE_VAR, package_name);
    context.insert(VERSION_VAR, version);
    context
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_body_template_is_rendered() {
        let remote = Remote {
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            link: "link".to_string(),
            contributors: vec![],
        };
        let body =
            release_body_from_template("my_package", "0.1.0", "my changes", &remote, None).unwrap();
        assert_eq!(body, "my changes");
    }

    #[test]
    fn default_tag_template_single_package() {
        let template = default_tag_name_template(false);
        assert_eq!(template, "v{{ version }}");
    }

    #[test]
    fn default_tag_template_multi_package() {
        let template = default_tag_name_template(true);
        assert_eq!(template, "{{ package }}-v{{ version }}");
    }

    #[test]
    fn template_renders_package_and_version() {
        let template = "{{ package }}-v{{ version }}";
        let context = tera_context("mylib", "1.2.3");
        let result = render_template(template, &context, "test").unwrap();
        assert_eq!(result, "mylib-v1.2.3");
    }

    #[test]
    fn template_renders_version_only() {
        let template = "v{{ version }}";
        let context = tera_context("ignored", "0.5.0");
        let result = render_template(template, &context, "test").unwrap();
        assert_eq!(result, "v0.5.0");
    }

    #[test]
    fn template_renders_custom_format() {
        let template = "release-{{ package }}-{{ version }}-prod";
        let context = tera_context("api", "2.0.0");
        let result = render_template(template, &context, "test").unwrap();
        assert_eq!(result, "release-api-2.0.0-prod");
    }
}
