use std::{process::Command, time::Duration};

use crate::helpers::gitea::CARGO_INDEX_REPO;
use assert_cmd::assert::Assert;
use cargo_metadata::{
    Package,
    camino::{Utf8Path, Utf8PathBuf},
    semver::Version,
};
use cargo_utils::{CARGO_TOML, LocalManifest};
use git_cmd::Repo;
use release_plz_core::{
    DEFAULT_BRANCH_PREFIX, GitClient, GitForge, GitPr, Gitea, Pr, RepoUrl,
    fs_utils::{Utf8TempDir, canonicalize_utf8},
};
use secrecy::SecretString;

use tracing::info;

use super::{
    TEST_REGISTRY, fake_utils,
    gitea::{GiteaContext, gitea_address},
    package::TestPackage,
};

const CRATES_DIR: &str = "crates";
const RELEASE_PLZ_LOG: &str = "RELEASE_PLZ_LOG";

/// It contains the universe in which release-plz runs.
pub struct TestContext {
    pub gitea: GiteaContext,
    test_dir: Utf8TempDir,
    /// Release-plz git client. It's here just for code reuse.
    git_client: GitClient,
    is_workspace: bool,
    pub repo: Repo,
}

impl TestContext {
    async fn init_context(is_workspace: bool) -> Self {
        test_logs::init();
        let repo_name = fake_utils::fake_id();
        let gitea = GiteaContext::new(repo_name).await;
        let test_dir = Utf8TempDir::new().unwrap();
        info!("test directory: {:?}", test_dir.path());
        let repo_url = gitea.repo_clone_url();
        git_clone(test_dir.path(), &repo_url);

        let git_client = git_client(&repo_url, &gitea.token);

        let repo_dir = test_dir.path().join(&gitea.repo);
        let repo = configure_repo(&repo_dir, &gitea);
        Self {
            gitea,
            test_dir,
            git_client,
            is_workspace,
            repo,
        }
    }

    pub fn package_path(&self, package_name: &str) -> Utf8PathBuf {
        if self.is_workspace {
            self.repo_dir().join(CRATES_DIR).join(package_name)
        } else {
            // If it's not a workspace, the package is at the root of the repo.
            self.repo_dir()
        }
    }

    pub fn set_package_version(&self, package_name: &str, version: &Version) {
        let mut manifest =
            LocalManifest::try_new(&self.package_path(package_name).join(CARGO_TOML)).unwrap();
        manifest.set_package_version(version);
        manifest.write().unwrap();
    }

    pub fn push_all_changes(&self, commit_message: &str) {
        self.repo.add_all_and_commit(commit_message).unwrap();
        self.repo.git(&["push"]).unwrap();
    }

    pub async fn push_to_pr(&self, commit_message: &str, branch: &str) {
        self.repo.git(&["checkout", "-b", branch]).unwrap();
        self.repo.add_all_and_commit(commit_message).unwrap();
        self.repo.git(&["push", "origin", branch]).unwrap();
        let pr = Pr {
            base_branch: "main".to_string(),
            branch: branch.to_string(),
            title: commit_message.to_string(),
            body: "This is my pull request".to_string(),
            draft: false,
            labels: vec![],
        };
        self.git_client.open_pr(&pr).await.unwrap();
        // go back to main
        self.repo.git(&["checkout", "-"]).unwrap();
    }

    pub async fn new() -> Self {
        let context = Self::init_context(false).await;
        let package = TestPackage::new(&context.gitea.repo);
        package.cargo_init(context.repo.directory());
        context.run_cargo_check();
        context.push_all_changes("cargo init");
        context
    }

    pub async fn new_workspace(crates: &[&str]) -> Self {
        let packages: Vec<TestPackage> = crates.iter().map(TestPackage::new).collect();
        Self::new_workspace_with_packages(&packages).await
    }

    pub async fn new_workspace_with_packages(crates: &[TestPackage]) -> Self {
        let context = Self::init_context(true).await;
        let root_cargo_toml = {
            let quoted_crates: Vec<String> = crates
                .iter()
                .map(|c| format!("\"{CRATES_DIR}/{}\"", &c.name))
                .collect();
            let crates_list = quoted_crates.join(",");
            format!("[workspace]\nresolver = \"3\"\nmembers = [{crates_list}]\n")
        };
        fs_err::write(context.repo.directory().join("Cargo.toml"), root_cargo_toml).unwrap();

        for package in crates {
            let crate_dir = context.package_path(&package.name);
            fs_err::create_dir_all(&crate_dir).unwrap();
            package.cargo_init(&crate_dir);
        }

        // add dependencies after all writing all Cargo.toml files
        for package in crates {
            let crate_dir = context.package_path(&package.name);
            package.write_dependencies(&crate_dir);
        }

        context.run_cargo_check();
        context.push_all_changes("cargo init");
        context
    }

    pub async fn merge_all_prs(&self) {
        let opened_prs = self.git_client.opened_prs("").await.unwrap();
        for pr in opened_prs {
            self.gitea.merge_pr_retrying(pr.number).await;
        }
        self.repo.git(&["pull"]).unwrap();
    }

    pub async fn merge_release_pr(&self) {
        let opened_prs = self.opened_release_prs().await;
        assert_eq!(opened_prs.len(), 1);
        self.gitea.merge_pr_retrying(opened_prs[0].number).await;
        self.repo.git(&["pull"]).unwrap();
    }

    /// Running this will create the Cargo.lock file if missing.
    pub fn run_cargo_check(&self) {
        assert_cmd::Command::new("cargo")
            .current_dir(self.repo.directory())
            .arg("check")
            .assert()
            .success();
    }

    pub fn run_update(&self) -> Assert {
        super::cmd::release_plz_cmd()
            .current_dir(self.repo_dir())
            .env(RELEASE_PLZ_LOG, log_level())
            .arg("update")
            .arg("--verbose")
            .arg("--registry")
            .arg(TEST_REGISTRY)
            .assert()
    }

    pub fn run_release_pr(&self) -> Assert {
        super::cmd::release_plz_cmd()
            .current_dir(self.repo_dir())
            .env(RELEASE_PLZ_LOG, log_level())
            .arg("release-pr")
            .arg("--verbose")
            .arg("--git-token")
            .arg(&self.gitea.token)
            .arg("--forge")
            .arg("gitea")
            .arg("--registry")
            .arg(TEST_REGISTRY)
            .arg("--output")
            .arg("json")
            .timeout(Duration::from_secs(300))
            .assert()
    }

    pub fn run_release(&self) -> Assert {
        super::cmd::release_plz_cmd()
            .current_dir(self.repo_dir())
            .env(RELEASE_PLZ_LOG, log_level())
            .arg("release")
            .arg("--verbose")
            .arg("--git-token")
            .arg(&self.gitea.token)
            .arg("--forge")
            .arg("gitea")
            .arg("--registry")
            .arg(TEST_REGISTRY)
            .arg("--token")
            .arg(format!("Bearer {}", &self.gitea.token))
            .arg("--output")
            .arg("json")
            .timeout(Duration::from_secs(300))
            .assert()
    }

    pub fn repo_dir(&self) -> Utf8PathBuf {
        let path = self.test_dir.path().join(&self.gitea.repo);
        canonicalize_utf8(&path).unwrap()
    }

    pub async fn opened_release_prs(&self) -> Vec<GitPr> {
        self.git_client
            .opened_prs(DEFAULT_BRANCH_PREFIX)
            .await
            .unwrap()
    }

    pub fn write_release_plz_toml(&self, content: &str) {
        let release_plz_toml_path = self.repo_dir().join("release-plz.toml");
        fs_err::write(release_plz_toml_path, content).unwrap();
        self.push_all_changes("add config file");
    }

    pub fn write_changelog(&self, content: &str) {
        let changelog_path = self.repo_dir().join("CHANGELOG.md");
        fs_err::write(changelog_path, content).unwrap();
        self.push_all_changes("edit changelog");
    }

    pub fn read_changelog(&self) -> String {
        let changelog_path = self.repo_dir().join("CHANGELOG.md");
        fs_err::read_to_string(changelog_path).unwrap()
    }

    pub fn download_package(&self, dest_dir: &Utf8Path) -> Vec<Package> {
        let crate_name = &self.gitea.repo;
        release_plz_core::PackageDownloader::new([crate_name], dest_dir.as_str())
            .with_registry(TEST_REGISTRY.to_string())
            .with_cargo_cwd(self.repo_dir())
            .download()
            .unwrap()
    }
}

pub fn run_set_version(directory: &Utf8Path, change: &str) {
    let change: Vec<_> = change.split(' ').collect();
    super::cmd::release_plz_cmd()
        .current_dir(directory)
        .env(RELEASE_PLZ_LOG, log_level())
        .arg("set-version")
        .args(&change)
        .assert();
}

fn log_level() -> String {
    if std::env::var("ENABLE_LOGS").is_ok() {
        std::env::var(RELEASE_PLZ_LOG).unwrap_or("DEBUG,hyper=INFO".to_string())
    } else {
        "ERROR".to_string()
    }
}

fn configure_repo(repo_dir: &Utf8Path, gitea: &GiteaContext) -> Repo {
    let username = gitea.user.username();
    let repo = Repo::new(repo_dir).unwrap();
    // config local user
    repo.git(&["config", "user.name", username]).unwrap();
    // set email
    repo.git(&["config", "user.email", &gitea.user.email()])
        .unwrap();
    // disable GPG signing for tests
    repo.git(&["config", "commit.gpgsign", "false"]).unwrap();

    create_cargo_config(repo_dir, username);

    repo
}

fn create_cargo_config(repo_dir: &Utf8Path, username: &str) {
    let config_dir = repo_dir.join(".cargo");
    fs_err::create_dir(&config_dir).unwrap();
    let config_file = config_dir.join("config.toml");
    let cargo_config = cargo_config(username);
    fs_err::write(config_file, cargo_config).unwrap();
}

fn cargo_config(username: &str) -> String {
    // matches the docker compose file
    let cargo_registries = format!(
        "[registry]\ndefault = \"{TEST_REGISTRY}\"\n\n[registries.{TEST_REGISTRY}]\nindex = "
    );
    // we use gitea as a cargo registry:
    // https://docs.gitea.com/usage/packages/cargo
    let gitea_index = format!(
        "\"http://{}/{}/{CARGO_INDEX_REPO}.git\"",
        gitea_address(),
        username
    );

    let config_end = r"
[net]
git-fetch-with-cli = true
    ";
    format!("{cargo_registries}{gitea_index}{config_end}")
}

fn git_client(repo_url: &str, token: &str) -> GitClient {
    let git_forge = GitForge::Gitea(
        Gitea::new(
            RepoUrl::new(repo_url).unwrap(),
            SecretString::from(token.to_string()),
        )
        .unwrap(),
    );
    GitClient::new(git_forge).unwrap()
}

fn git_clone(path: &Utf8Path, repo_url: &str) {
    let result = Command::new("git")
        .current_dir(path)
        .arg("clone")
        .arg(repo_url)
        .spawn()
        .unwrap()
        .wait()
        .unwrap();
    assert!(result.success());
}
