use crate::helpers::github_mock_server::GitHubMockServer;
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use cargo_metadata::Metadata;
use cargo_utils::{get_manifest_metadata, CARGO_TOML};
use git_cmd::Repo;
use release_plz_core::{
    fs_utils::Utf8TempDir, GitBackend, GitHub, GitRelease, GitReleaseConfig, PackagesUpdate,
    PublishConfig, Release, ReleaseConfig, ReleaseRequest, UpdateConfig, UpdateRequest,
};
use secrecy::SecretString;
use std::ffi::OsStr;
use std::process::Command;

pub struct GitOnlyTestContext {
    pub repo: Repo,
    github_mock_server: GitHubMockServer,
    _test_dir: Utf8TempDir,
}

pub const PROJECT_NAME: &str = "myproject";
const PROJECT_UPSTREAM_NAME: &str = "myproject-upstream";
const OWNER: &str = "owner";
const REPO: &str = "repo";

impl GitOnlyTestContext {
    pub async fn new() -> Self {
        let context = Self::init().await;

        cargo_init(context.project_dir());
        context.generate_cargo_lock();
        context.add_all_commit_and_push("feat: Initial commit");

        context
    }

    async fn init() -> Self {
        test_logs::init();
        let test_dir = Utf8TempDir::new().unwrap();

        // Create upstream repo and clone it into project dir
        let upstream_dir = test_dir.path().join(PROJECT_UPSTREAM_NAME);
        fs_err::create_dir(&upstream_dir).unwrap();
        let upstream_repo = Repo::init(&upstream_dir);
        // Checkout detached HEAD so we can receive updates to master branch
        upstream_repo
            .checkout(&upstream_repo.current_commit_hash().unwrap())
            .unwrap();

        let project_dir = test_dir.path().join(PROJECT_NAME);
        fs_err::create_dir(&project_dir).unwrap();
        git_cmd::git_in_dir(
            &project_dir,
            &["clone", upstream_dir.as_str(), project_dir.as_str()],
        )
        .unwrap();
        let repo = Repo::new(&project_dir).unwrap();
        // Set author details
        repo.git(&["config", "user.name", "author_name"]).unwrap();
        repo.git(&["config", "user.email", "author@example.com"])
            .unwrap();

        Self {
            _test_dir: test_dir,
            github_mock_server: GitHubMockServer::start(OWNER, REPO).await,
            repo,
        }
    }

    pub async fn run_update(&self) -> anyhow::Result<PackagesUpdate> {
        let update_request = self.update_request();
        release_plz_core::update(&update_request)
            .await
            .map(|(packages_update, _)| packages_update)
    }

    fn update_request(&self) -> UpdateRequest {
        // TODO: Git tag configuration

        UpdateRequest::new(self.workspace_metadata())
            .unwrap()
            .with_default_package_config(UpdateConfig {
                release: true,
                git_only: true,
                ..Default::default()
            })
    }

    fn workspace_metadata(&self) -> Metadata {
        get_manifest_metadata(&self.manifest_path()).unwrap()
    }

    pub async fn run_release(&self) -> anyhow::Result<Option<Release>> {
        let release_request = self.release_request();
        release_plz_core::release(&release_request).await
    }

    fn release_request(&self) -> ReleaseRequest {
        // TODO: Git tag configuration

        let config = ReleaseConfig::default()
            .with_publish(PublishConfig::enabled(false))
            .with_git_release(GitReleaseConfig::enabled(false))
            .with_git_only(true);

        ReleaseRequest::new(self.workspace_metadata())
            .with_default_package_config(config)
            .with_git_release(GitRelease {
                backend: self.git_backend(),
            })
    }

    fn git_backend(&self) -> GitBackend {
        GitBackend::Github(
            GitHub::new(
                OWNER.to_string(),
                REPO.to_string(),
                SecretString::from("token".to_string()),
            )
            .with_base_url(self.github_mock_server.base_url()),
        )
    }

    pub fn project_dir(&self) -> &Utf8Path {
        self.repo.directory()
    }

    pub fn manifest_path(&self) -> Utf8PathBuf {
        self.project_dir().join(CARGO_TOML)
    }

    fn generate_cargo_lock(&self) {
        Command::new("cargo")
            .current_dir(self.repo.directory())
            .arg("check")
            .output()
            .unwrap();
    }

    pub fn add_all_commit_and_push(&self, message: impl AsRef<str>) {
        self.repo.add_all_and_commit(message.as_ref()).unwrap();
        self.repo.git(&["push"]).unwrap();
    }
}

fn cargo_init(dir: impl AsRef<OsStr>) {
    Command::new("cargo").arg("init").arg(dir).output().unwrap();
}
