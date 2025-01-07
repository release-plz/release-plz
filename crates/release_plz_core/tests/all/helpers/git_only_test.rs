use crate::helpers::github_mock_server::GitHubMockServer;
use cargo::core::Workspace;
use cargo::GlobalContext;
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use cargo_metadata::Metadata;
use cargo_utils::{get_manifest_metadata, LocalManifest, CARGO_TOML};
use git_cmd::Repo;
use itertools::Itertools;
use release_plz_core::{
    fs_utils::Utf8TempDir, GitBackend, GitHub, GitRelease, GitReleaseConfig, GitTagConfig,
    PackagesUpdate, PublishConfig, Release, ReleaseConfig, ReleaseRequest, UpdateConfig,
    UpdateRequest,
};
use secrecy::SecretString;
use std::ffi::OsStr;
use std::process::Command;

pub struct GitOnlyTestContext {
    pub repo: Repo,
    github_mock_server: GitHubMockServer,
    tag_template: Option<String>,
    _test_dir: Utf8TempDir,
    gctx: GlobalContext,
}

pub const PROJECT_NAME: &str = "myproject";
const PROJECT_UPSTREAM_NAME: &str = "myproject-upstream";
const OWNER: &str = "owner";
const REPO: &str = "repo";
const SPARSE_CRATES_IO_REGISTRY: &str = "crates-io-sparse";

impl GitOnlyTestContext {
    pub async fn new(tag_template: Option<String>) -> Self {
        let context = Self::init(tag_template).await;

        cargo_init(context.project_dir());
        context.generate_cargo_lock();
        context.add_all_commit_and_push("feat: Initial commit");

        context
    }

    pub async fn new_workspace<S: AsRef<Utf8Path>, const N: usize>(
        tag_template: Option<String>,
        crates: [S; N],
    ) -> (Self, [Utf8PathBuf; N]) {
        let context = Self::init(tag_template).await;

        let root_cargo_toml = {
            let crates_list = crates
                .iter()
                .format_with(", ", |c, fmt| fmt(&format_args!("\"{}\"", c.as_ref())));
            format!("[workspace]\nresolver = \"2\"\nmembers = [{crates_list}]\n")
        };
        fs_err::write(context.project_dir().join(CARGO_TOML), root_cargo_toml).unwrap();

        let crate_dirs = crates.map(|crate_dir| crate_dir.as_ref().into());

        for crate_dir in &crate_dirs {
            let crate_dir = context.crate_dir(crate_dir);
            fs_err::create_dir_all(&crate_dir).unwrap();
            cargo_init(crate_dir);
        }

        context.generate_cargo_lock();
        context.add_all_commit_and_push("feat: Initial commit");
        (context, crate_dirs)
    }

    async fn init(tag_template: Option<String>) -> Self {
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
        // Set autocrlf to false so we use the system line endings
        repo.git(&["config", "core.autocrlf", "false"]).unwrap();

        // Add sparse crates.io registry to cargo config
        let mut cargo_config = toml_edit::DocumentMut::new();
        cargo_config.insert(
            "registries",
            toml_edit::Item::Table(toml_edit::Table::from_iter([(
                SPARSE_CRATES_IO_REGISTRY,
                toml_edit::InlineTable::from_iter([("index", crates_index::sparse::URL)]),
            )])),
        );

        let cargo_config_path = project_dir.join(".cargo/config.toml");
        fs_err::create_dir(cargo_config_path.parent().unwrap()).unwrap();
        fs_err::write(cargo_config_path, cargo_config.to_string()).unwrap();

        let shell = cargo::core::Shell::new();
        let homedir = cargo::util::homedir(project_dir.as_std_path()).unwrap();

        Self {
            _test_dir: test_dir,
            github_mock_server: GitHubMockServer::start(OWNER, REPO).await,
            tag_template,
            repo,
            gctx: GlobalContext::new(shell, project_dir.into_std_path_buf(), homedir),
        }
    }

    pub async fn run_update_and_commit(&self) -> anyhow::Result<PackagesUpdate> {
        self.run_update().await.inspect(|_| {
            if self.repo.is_clean().is_err() {
                self.add_all_commit_and_push("chore: release");
            }
        })
    }

    pub async fn run_update(&self) -> anyhow::Result<PackagesUpdate> {
        let update_request = self.update_request();
        release_plz_core::update(&update_request)
            .await
            .map(|(packages_update, _)| packages_update)
    }

    fn update_request(&self) -> UpdateRequest {
        UpdateRequest::new(self.workspace_metadata())
            .unwrap()
            .with_default_package_config(UpdateConfig {
                release: true,
                tag_name_template: self.tag_template.clone(),
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
        let config = ReleaseConfig::default()
            .with_publish(PublishConfig::enabled(false))
            .with_git_release(GitReleaseConfig::enabled(false))
            .with_git_tag(GitTagConfig::enabled(true).set_name_template(self.tag_template.clone()));

        ReleaseRequest::new(self.workspace_metadata())
            .with_default_package_config(config)
            // Specify a sparse registry by default - tests don't interact with it since
            // we don't publish any packages anyway
            .with_registry(SPARSE_CRATES_IO_REGISTRY)
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

    pub fn write_cargo_toml(
        &self,
        crate_dir: impl AsRef<Utf8Path>,
        write_fn: impl FnOnce(&mut toml_edit::DocumentMut),
    ) -> anyhow::Result<()> {
        let mut cargo_toml_dir = self.crate_dir(crate_dir);
        cargo_toml_dir.push(CARGO_TOML);
        write_cargo_toml(&cargo_toml_dir, write_fn)
    }

    pub fn write_root_cargo_toml(
        &self,
        write_fn: impl FnOnce(&mut toml_edit::DocumentMut),
    ) -> anyhow::Result<()> {
        write_cargo_toml(&self.project_dir().join(CARGO_TOML), write_fn)
    }

    pub fn crate_dir(&self, crate_dir: impl AsRef<Utf8Path>) -> Utf8PathBuf {
        self.project_dir().join(crate_dir)
    }

    pub fn cargo_gctx(&self) -> &GlobalContext {
        &self.gctx
    }

    pub fn cargo_workspace(&self) -> Workspace {
        Workspace::new(self.manifest_path().as_std_path(), self.cargo_gctx()).unwrap()
    }

    pub fn run_cargo_update(
        &self,
        to_update: Vec<String>,
        precise: Option<&str>,
    ) -> anyhow::Result<()> {
        let workspace = self.cargo_workspace();
        let update_options = cargo::ops::UpdateOptions {
            gctx: workspace.gctx(),
            to_update,
            precise,
            recursive: false,
            dry_run: false,
            workspace: false,
        };
        cargo::ops::update_lockfile(&workspace, &update_options)
    }
}

fn write_cargo_toml(
    cargo_toml_dir: &Utf8PathBuf,
    write_fn: impl FnOnce(&mut toml_edit::DocumentMut),
) -> anyhow::Result<()> {
    let mut cargo_toml = LocalManifest::try_new(cargo_toml_dir)?;
    write_fn(&mut cargo_toml.data);
    cargo_toml.write()
}

fn cargo_init(dir: impl AsRef<OsStr>) {
    Command::new("cargo").arg("init").arg(dir).output().unwrap();
}
