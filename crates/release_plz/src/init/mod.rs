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
    store_cargo_token()?;

    enable_pr_permissions(&repo_url)?;
    let github_token = store_github_token()?;
    write_actions_yaml(github_token)?;

    print_recap(&repo_url);
    Ok(())
}

fn actions_file_parent() -> Utf8PathBuf {
    Utf8Path::new(".github").join("workflows")
}

fn actions_file() -> Utf8PathBuf {
    actions_file_parent().join("release-plz.yml")
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
    )?;

    let github_token = if should_create_token {
        println!("
👉 Paste your GitHub PAT.
💡 Create a GitHub PAT following these instructions: https://release-plz.dev/docs/github/token#use-a-personal-access-token");

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

fn print_recap(repo_url: &str) {
    println!(
        "All done 🎉
- GitHub action file written to {}
- GitHub action secrets stored. Review them at {}

Enjoy automated releases 🤖",
        actions_file(),
        actions_secret_url(repo_url)
    );
}

fn read_stdin() -> anyhow::Result<String> {
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context("error while reading user input")?;
    Ok(input)
}

fn ask_confirmation(question: &str) -> anyhow::Result<bool> {
    print!("{question} (Y/n) ");
    std::io::stdout().flush().unwrap();
    let input = read_stdin()?;
    let input = input.trim().to_lowercase();
    Ok(input != "n")
}

fn write_actions_yaml(github_token: &str) -> anyhow::Result<()> {
    let branch = gh::default_branch()?;
    let action_yaml = action_yaml(&branch, github_token);
    fs_err::create_dir_all(actions_file_parent())
        .context("failed to create GitHub actions workflows directory")?;
    fs_err::write(actions_file(), action_yaml).context("error while writing GitHub action file")?;
    Ok(())
}

fn action_yaml(branch: &str, github_token: &str) -> String {
    let github_token_secret = format!("${{{{ secrets.{github_token} }}}}");
    let is_default_token = github_token == GITHUB_TOKEN;
    let checkout_token_line = if is_default_token {
        "".to_string()
    } else {
        format!(
            "
          token: {github_token_secret}"
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
    permissions:
      contents: write
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4
        with:
          fetch-depth: 0{checkout_token_line}
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Run release-plz
        uses: release-plz/action@v0.5
        with:
          command: release
        env:
          GITHUB_TOKEN: {github_token_secret}
          CARGO_REGISTRY_TOKEN: ${{{{ secrets.{CARGO_REGISTRY_TOKEN} }}}}

  release-plz-pr:
    name: Release-plz PR
    runs-on: ubuntu-latest
    permissions:
      pull-requests: write
      contents: write
    concurrency:
      group: release-plz-${{{{ github.ref }}}}
      cancel-in-progress: false
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4
        with:
          fetch-depth: 0{checkout_token_line}
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Run release-plz
        uses: release-plz/action@v0.5
        with:
          command: release-pr
        env:
          GITHUB_TOKEN: {github_token_secret}
          CARGO_REGISTRY_TOKEN: ${{{{ secrets.{CARGO_REGISTRY_TOKEN} }}}}
"
    )
}

fn ensure_gh_is_installed() -> anyhow::Result<()> {
    anyhow::ensure!(
        gh::is_gh_installed(),
        "❌ gh cli is not installed. I need it to store GitHub actions repository secrets. Please install it from https://docs.github.com/en/github-cli/github-cli/quickstart"
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
                permissions:
                  contents: write
                steps:
                  - name: Checkout repository
                    uses: actions/checkout@v4
                    with:
                      fetch-depth: 0
                  - name: Install Rust toolchain
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
                permissions:
                  pull-requests: write
                  contents: write
                concurrency:
                  group: release-plz-${{ github.ref }}
                  cancel-in-progress: false
                steps:
                  - name: Checkout repository
                    uses: actions/checkout@v4
                    with:
                      fetch-depth: 0
                  - name: Install Rust toolchain
                    uses: dtolnay/rust-toolchain@stable
                  - name: Run release-plz
                    uses: release-plz/action@v0.5
                    with:
                      command: release-pr
                    env:
                      GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
                      CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        "#]]
        .assert_eq(&action_yaml("main", GITHUB_TOKEN));
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
                permissions:
                  contents: write
                steps:
                  - name: Checkout repository
                    uses: actions/checkout@v4
                    with:
                      fetch-depth: 0
                      token: ${{ secrets.RELEASE_PLZ_TOKEN }}
                  - name: Install Rust toolchain
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
                permissions:
                  pull-requests: write
                  contents: write
                concurrency:
                  group: release-plz-${{ github.ref }}
                  cancel-in-progress: false
                steps:
                  - name: Checkout repository
                    uses: actions/checkout@v4
                    with:
                      fetch-depth: 0
                      token: ${{ secrets.RELEASE_PLZ_TOKEN }}
                  - name: Install Rust toolchain
                    uses: dtolnay/rust-toolchain@stable
                  - name: Run release-plz
                    uses: release-plz/action@v0.5
                    with:
                      command: release-pr
                    env:
                      GITHUB_TOKEN: ${{ secrets.RELEASE_PLZ_TOKEN }}
                      CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        "#]]
        .assert_eq(&action_yaml("main", CUSTOM_GITHUB_TOKEN));
    }
}
