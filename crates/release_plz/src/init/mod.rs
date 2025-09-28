mod gh;

use std::io::Write;

use anyhow::Context;
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use release_plz_core::{Project, ReleaseMetadata, ReleaseMetadataBuilder};
use std::collections::HashSet;

const CARGO_REGISTRY_TOKEN: &str = "CARGO_REGISTRY_TOKEN";
const GITHUB_TOKEN: &str = "GITHUB_TOKEN";
const CUSTOM_GITHUB_TOKEN: &str = "RELEASE_PLZ_TOKEN";

pub fn init(manifest_path: &Utf8Path, toml_check: bool) -> anyhow::Result<()> {
    ensure_gh_is_installed()?;

    // Create a Project instance to check mandatory fields
    let metadata = cargo_utils::get_manifest_metadata(manifest_path)?;
    let project = Project::new(
        manifest_path,
        None,
        &HashSet::new(),
        &metadata,
        &NoopReleaseMetadataBuilder,
    )?;

    if toml_check {
        project.check_mandatory_fields()?;
    }

    // get the repo url early to verify that the github repository is configured correctly
    let repo_url = gh::repo_url()?;

    greet();
    let trusted_publishing = should_use_trusted_publishing()?;
    if trusted_publishing {
        print_settings_urls(&project)?;
    } else {
        store_cargo_token()?;
    }

    let tag_signing = should_use_tag_signing()?;

    enable_pr_permissions(&repo_url)?;
    let github_token = store_github_token()?;
    write_actions_yaml(github_token, trusted_publishing, tag_signing)?;

    let secrets_stored = !trusted_publishing || github_token != GITHUB_TOKEN;
    print_recap(&repo_url, secrets_stored);
    Ok(())
}

fn actions_file_parent() -> Utf8PathBuf {
    Utf8Path::new(".github").join("workflows")
}

fn actions_file() -> Utf8PathBuf {
    actions_file_parent().join("release-plz.yml")
}

fn should_use_trusted_publishing() -> anyhow::Result<bool> {
    ask_confirmation(
        "👉 Do you want to use trusted publishing? (Recommended). Learn more at https://crates.io/docs/trusted-publishing.",
        true,
    )
}

fn should_use_tag_signing() -> anyhow::Result<bool> {
    ask_confirmation(
        "👉 Do you want to enable tag signing? (Not recommended). Learn more at https://release-plz.dev/docs/github/persist-credentials.",
        false,
    )
}

fn print_settings_urls(project: &Project) -> anyhow::Result<()> {
    println!(
        "Enable trusted publishing for your crates. Note:
- The default workflow name is `release-plz.yml`.
- If you use an environment, edit the final workflow file.

Settings URLs:"
    );

    let publishable_packages = &project.publishable_packages();
    let settings_urls = publishable_packages.iter().map(|package| {
        let package_name = &package.name;
        format!("https://crates.io/crates/{package_name}/settings/new-trusted-publisher")
    });

    for url in settings_urls {
        println!("* {url}");
    }
    println!("\nType Enter when done.");

    read_stdin()?;

    Ok(())
}

fn greet() {
    println!(
        "👋 This process will guide you in setting up release-plz in your GitHub repository, using `gh` (the GitHub CLI) to store the necessary tokens in your repository secrets."
    );
}

fn store_cargo_token() -> anyhow::Result<()> {
    println!("👉 Paste your cargo registry token to store it in the GitHub actions repository secrets.
💡 You can create a crates.io token on https://crates.io/settings/tokens/new, specifying the following scopes: \"publish-new\" and \"publish-update\".");
    gh::store_secret(CARGO_REGISTRY_TOKEN)?;
    Ok(())
}

fn enable_pr_permissions(repo_url: &str) -> anyhow::Result<()> {
    println!("
👉 Go to {} and enable the option \"Allow GitHub Actions to create and approve pull requests\". Type Enter when done.", actions_settings_url(repo_url));
    read_stdin()?;
    Ok(())
}

fn store_github_token() -> anyhow::Result<&'static str> {
    let should_create_token = ask_confirmation(
        "👉 Do you want release-plz to use a GitHub Personal Access Token (PAT)? It's required to run CI on release PRs and to run workflows on tags.",
        true,
    )?;

    let github_token = if should_create_token {
        println!("
👉 Paste your GitHub PAT.
💡 Create a GitHub PAT following these instructions:

   1. Go to https://github.com/settings/personal-access-tokens/new.
   2. Under \"Only selected repositories\", select the repositories where you want to use the PAT, to give release-plz write access.
   3. Under \"Repository permissions\", assign \"Contents\" and \"Pull requests\" read and write permissions.

   If you have doubts, check the documentation: https://release-plz.dev/docs/github/token#use-a-personal-access-token.");

        // GitHub custom token
        let release_plz_token: &str = CUSTOM_GITHUB_TOKEN;
        gh::store_secret(release_plz_token)?;
        release_plz_token
    } else {
        // default github token
        GITHUB_TOKEN
    };
    Ok(github_token)
}

fn print_recap(repo_url: &str, secrets_stored: bool) {
    println!(
        "All done 🎉
- GitHub action file written to {}",
        actions_file()
    );

    if secrets_stored {
        println!(
            "- GitHub action secrets stored. Review them at {}",
            actions_secret_url(repo_url)
        );
    }

    println!("Enjoy automated releases 🤖");
}

fn read_stdin() -> anyhow::Result<String> {
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context("error while reading user input")?;
    Ok(input)
}

fn ask_confirmation(question: &str, default: bool) -> anyhow::Result<bool> {
    print!(
        "{question} ({}/{}) ",
        if default { "Y" } else { "y" },
        if default { "n" } else { "N" }
    );
    std::io::stdout().flush().unwrap();
    let input = read_stdin()?;
    let input = input.trim().to_lowercase();
    Ok(input != if default { "n" } else { "y" })
}

fn write_actions_yaml(
    github_token: &str,
    trusted_publishing: bool,
    tag_signing: bool,
) -> anyhow::Result<()> {
    let branch = gh::default_branch()?;
    let owner = gh::repo_owner()?;
    let action_yaml = action_yaml(
        &branch,
        github_token,
        &owner,
        trusted_publishing,
        tag_signing,
    );
    fs_err::create_dir_all(actions_file_parent())
        .context("failed to create GitHub actions workflows directory")?;
    fs_err::write(actions_file(), action_yaml).context("error while writing GitHub action file")?;
    Ok(())
}

fn action_yaml(
    branch: &str,
    github_token: &str,
    owner: &str,
    trusted_publishing: bool,
    tag_signing: bool,
) -> String {
    let github_token_secret = format!("${{{{ secrets.{github_token} }}}}");
    let is_default_token = github_token == GITHUB_TOKEN;
    let checkout_token_line = if is_default_token || tag_signing {
        "".to_string()
    } else {
        format!(
            "
          token: {github_token_secret}"
        )
    };

    let cargo_registry_token = if trusted_publishing {
        // release-plz will generate the token during the release process if needed,
        // so we don't need to pass the token to the action.
        String::new()
    } else {
        format!("${{{{ secrets.{CARGO_REGISTRY_TOKEN} }}}}")
    };

    let id_token_permissions = if trusted_publishing {
        "
      id-token: write"
    } else {
        ""
    };

    let pr_cargo_registry_token = if trusted_publishing {
        // For public crates, the cargo registry token is not needed in the PR workflow.
        // So if we use trusted publishing, we can omit it.
        // Trusted publishing also works for private crates, and if that's your case, write the token manually.
        "".to_string()
    } else {
        // The crate might be private, so we add the token to the PR workflow.
        // If the crate is public, it won't hurt having it here. You can also remove it if you want.
        format!(
            "
          CARGO_REGISTRY_TOKEN: {cargo_registry_token}"
        )
    };

    let release_cargo_registry_token_env = if trusted_publishing {
        // Omit the token in the release workflow as well when using trusted publishing.
        "".to_string()
    } else {
        format!(
            "
          CARGO_REGISTRY_TOKEN: {cargo_registry_token}"
        )
    };

    format!(
        "name: Release-plz

on:
  push:
    branches:
      - {branch}

jobs:
  release-plz-release:
    name: Release-plz release
    runs-on: ubuntu-latest
    if: ${{{{ github.repository_owner == '{owner}' }}}}
    permissions:
      contents: write{id_token_permissions}
    steps:
      - &checkout
        name: Checkout repository
        uses: actions/checkout@v5
        with:
          fetch-depth: 0
          persist-credentials: {tag_signing}{checkout_token_line}
      - &install-rust
        name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Run release-plz
        uses: release-plz/action@v0.5
        with:
          command: release
        env:
          GITHUB_TOKEN: {github_token_secret}{release_cargo_registry_token_env}

  release-plz-pr:
    name: Release-plz PR
    runs-on: ubuntu-latest
    if: ${{{{ github.repository_owner == '{owner}' }}}}
    permissions:
      pull-requests: write
      contents: write
    concurrency:
      group: release-plz-${{{{ github.ref }}}}
      cancel-in-progress: false
    steps:
      - *checkout
      - *install-rust
      - name: Run release-plz
        uses: release-plz/action@v0.5
        with:
          command: release-pr
        env:
          GITHUB_TOKEN: {github_token_secret}{pr_cargo_registry_token}
"
    )
}

fn ensure_gh_is_installed() -> anyhow::Result<()> {
    anyhow::ensure!(
        gh::is_gh_installed(),
        "❌ gh cli is not installed. I need it to store GitHub actions repository secrets. Please install it from https://docs.github.com/en/github-cli/github-cli/quickstart."
    );
    Ok(())
}

fn actions_settings_url(repo_url: &str) -> String {
    format!("{}/actions", repo_settings_url(repo_url))
}

fn actions_secret_url(repo_url: &str) -> String {
    format!("{}/secrets/actions", repo_settings_url(repo_url))
}

fn repo_settings_url(repo_url: &str) -> String {
    format!("{repo_url}/settings")
}

struct NoopReleaseMetadataBuilder;

impl ReleaseMetadataBuilder for NoopReleaseMetadataBuilder {
    fn get_release_metadata(&self, _package_name: &str) -> Option<ReleaseMetadata> {
        // This needs to be `Some`, otherwise release-plz doesn't find any public packages.
        Some(ReleaseMetadata {
            release_name_template: None,
            tag_name_template: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_yaml_string_is_correct() {
        expect_test::expect![[r#"
            name: Release-plz

            on:
              push:
                branches:
                  - main

            jobs:
              release-plz-release:
                name: Release-plz release
                runs-on: ubuntu-latest
                if: ${{ github.repository_owner == 'owner' }}
                permissions:
                  contents: write
                steps:
                  - &checkout
                    name: Checkout repository
                    uses: actions/checkout@v5
                    with:
                      fetch-depth: 0
                      persist-credentials: false
                  - &install-rust
                    name: Install Rust toolchain
                    uses: dtolnay/rust-toolchain@stable
                  - name: Run release-plz
                    uses: release-plz/action@v0.5
                    with:
                      command: release
                    env:
                      GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
                      CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}

              release-plz-pr:
                name: Release-plz PR
                runs-on: ubuntu-latest
                if: ${{ github.repository_owner == 'owner' }}
                permissions:
                  pull-requests: write
                  contents: write
                concurrency:
                  group: release-plz-${{ github.ref }}
                  cancel-in-progress: false
                steps:
                  - *checkout
                  - *install-rust
                  - name: Run release-plz
                    uses: release-plz/action@v0.5
                    with:
                      command: release-pr
                    env:
                      GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
                      CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        "#]]
        .assert_eq(&action_yaml("main", GITHUB_TOKEN, "owner", false, false));
    }

    #[test]
    fn actions_yaml_string_with_custom_token_is_correct() {
        expect_test::expect![[r#"
            name: Release-plz

            on:
              push:
                branches:
                  - main

            jobs:
              release-plz-release:
                name: Release-plz release
                runs-on: ubuntu-latest
                if: ${{ github.repository_owner == 'owner' }}
                permissions:
                  contents: write
                steps:
                  - &checkout
                    name: Checkout repository
                    uses: actions/checkout@v5
                    with:
                      fetch-depth: 0
                      persist-credentials: false
                  - &install-rust
                    name: Install Rust toolchain
                    uses: dtolnay/rust-toolchain@stable
                  - name: Run release-plz
                    uses: release-plz/action@v0.5
                    with:
                      command: release
                    env:
                      GITHUB_TOKEN: ${{ secrets.RELEASE_PLZ_TOKEN }}
                      CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}

              release-plz-pr:
                name: Release-plz PR
                runs-on: ubuntu-latest
                if: ${{ github.repository_owner == 'owner' }}
                permissions:
                  pull-requests: write
                  contents: write
                concurrency:
                  group: release-plz-${{ github.ref }}
                  cancel-in-progress: false
                steps:
                  - *checkout
                  - *install-rust
                  - name: Run release-plz
                    uses: release-plz/action@v0.5
                    with:
                      command: release-pr
                    env:
                      GITHUB_TOKEN: ${{ secrets.RELEASE_PLZ_TOKEN }}
                      CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        "#]]
        .assert_eq(&action_yaml(
            "main",
            CUSTOM_GITHUB_TOKEN,
            "owner",
            false,
            false,
        ));
    }
}

#[test]
fn actions_yaml_string_with_trusted_publishing_is_correct() {
    expect_test::expect![[r#"
            name: Release-plz

            on:
              push:
                branches:
                  - main

            jobs:
              release-plz-release:
                name: Release-plz release
                runs-on: ubuntu-latest
                if: ${{ github.repository_owner == 'owner' }}
                permissions:
                  contents: write
                  id-token: write
                steps:
                  - &checkout
                    name: Checkout repository
                    uses: actions/checkout@v5
                    with:
                      fetch-depth: 0
                      persist-credentials: false
                  - &install-rust
                    name: Install Rust toolchain
                    uses: dtolnay/rust-toolchain@stable
                  - name: Run release-plz
                    uses: release-plz/action@v0.5
                    with:
                      command: release
                    env:
                      GITHUB_TOKEN: ${{ secrets.RELEASE_PLZ_TOKEN }}

              release-plz-pr:
                name: Release-plz PR
                runs-on: ubuntu-latest
                if: ${{ github.repository_owner == 'owner' }}
                permissions:
                  pull-requests: write
                  contents: write
                concurrency:
                  group: release-plz-${{ github.ref }}
                  cancel-in-progress: false
                steps:
                  - *checkout
                  - *install-rust
                  - name: Run release-plz
                    uses: release-plz/action@v0.5
                    with:
                      command: release-pr
                    env:
                      GITHUB_TOKEN: ${{ secrets.RELEASE_PLZ_TOKEN }}
        "#]]
    .assert_eq(&action_yaml(
        "main",
        CUSTOM_GITHUB_TOKEN,
        "owner",
        true,
        false,
    ));
}

#[test]
fn actions_yaml_string_with_tag_signing_is_correct() {
    expect_test::expect![[r#"
            name: Release-plz

            on:
              push:
                branches:
                  - main

            jobs:
              release-plz-release:
                name: Release-plz release
                runs-on: ubuntu-latest
                if: ${{ github.repository_owner == 'owner' }}
                permissions:
                  contents: write
                steps:
                  - &checkout
                    name: Checkout repository
                    uses: actions/checkout@v5
                    with:
                      fetch-depth: 0
                      persist-credentials: true
                      token: ${{ secrets.RELEASE_PLZ_TOKEN }}
                  - &install-rust
                    name: Install Rust toolchain
                    uses: dtolnay/rust-toolchain@stable
                  - name: Run release-plz
                    uses: release-plz/action@v0.5
                    with:
                      command: release
                    env:
                      GITHUB_TOKEN: ${{ secrets.RELEASE_PLZ_TOKEN }}
                      CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}

              release-plz-pr:
                name: Release-plz PR
                runs-on: ubuntu-latest
                if: ${{ github.repository_owner == 'owner' }}
                permissions:
                  pull-requests: write
                  contents: write
                concurrency:
                  group: release-plz-${{ github.ref }}
                  cancel-in-progress: false
                steps:
                  - *checkout
                  - *install-rust
                  - name: Run release-plz
                    uses: release-plz/action@v0.5
                    with:
                      command: release-pr
                    env:
                      GITHUB_TOKEN: ${{ secrets.RELEASE_PLZ_TOKEN }}
                      CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        "#]]
    .assert_eq(&action_yaml(
        "main",
        CUSTOM_GITHUB_TOKEN,
        "owner",
        false,
        true,
    ));
}
