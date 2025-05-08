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
}
