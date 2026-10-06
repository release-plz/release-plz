//! Binary distribution using cargo-dist and draft GitHub release assets.
mod cargo_dist;
mod github;
#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::PathBuf,
};

use anyhow::{Context as _, ensure};
use cargo_metadata::Package;
use git_cmd::Repo;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ForgeType, GitClient, Project, ReleaseRequest};
use cargo_dist::CargoDist;
use github::{Asset, GitHubRelease};

/// A distribution is always tied to one package, tag and checked-out commit.
#[derive(Debug)]
pub struct DistRequest {
    request: ReleaseRequest,
    package: Package,
    project: Project,
    tag: String,
    commit: String,
    client: GitClient,
    repository: String,
    draft: bool,
    latest: Option<bool>,
}

impl DistRequest {
    pub fn new(request: ReleaseRequest, tag: String, client: GitClient) -> anyhow::Result<Self> {
        ensure!(client.forge == ForgeType::Github, "dist requires GitHub");
        // cargo-dist accepts only github.com, including when Git uses a public SSH alias,
        // so derive the canonical repository URL from the owner and name instead of the remote.
        let repository = format!(
            "https://github.com/{}/{}",
            client.remote.owner, client.remote.repo
        );
        let metadata = request.cargo_metadata();
        let project = Project::new(
            &metadata.workspace_root.join(cargo_utils::CARGO_TOML),
            None,
            &HashSet::new(),
            metadata,
            &request,
        )?;
        let mut packages = vec![];
        for package in project.workspace_packages() {
            if request.get_package_config(&package.name).distribute()
                && project.git_tag(&package.name, &package.version.to_string())? == tag
            {
                packages.push(package);
            }
        }
        let [package] = packages.as_slice() else {
            anyhow::bail!("tag `{tag}` must select exactly one package with distribute=true");
        };
        let package = (*package).clone();
        ensure!(
            package.targets.iter().any(|target| target.is_bin()),
            "dist requires a binary target"
        );
        let repo = Repo::new(&metadata.workspace_root)?;
        repo.is_clean()
            .context("distribution requires a clean checkout of the release tag")?;
        let commit = repo.current_commit_hash()?;
        let tagged_commit = repo
            .git(&[
                "rev-parse",
                "--verify",
                &format!("refs/tags/{tag}^{{commit}}"),
            ])
            .context("release tag is missing locally; check out the tag with fetch-depth: 0")?;
        ensure!(
            commit.trim() == tagged_commit.trim(),
            "HEAD must match release tag `{tag}`"
        );
        let package_config = request.get_package_config(&package.name);
        let git_release = package_config.git_release();
        Ok(Self {
            request,
            package,
            project,
            tag,
            commit,
            client,
            repository,
            draft: git_release.draft(),
            latest: git_release.latest(),
        })
    }

    /// Build for this runner and upload assets, then a receipt indicating successful completion.
    pub async fn build(&self, job: DistJob) -> anyhow::Result<()> {
        job.validate()?;
        self.require_draft_release().await?;
        let cargo_dist = self.cargo_dist(vec![job.target.clone()])?;
        let mut manifest = self.build_manifest(&cargo_dist, false)?;
        ensure!(
            manifest.has_binary_for(&job.target),
            "cargo-dist did not build binaries for {}",
            job.target
        );
        // The build is slow: a concurrent finalize may have published the release meanwhile.
        let release = self.require_draft_release().await?;
        let existing = self.client.dist_assets(&release).await?;
        let assets = self
            .upload_manifest_artifacts(&cargo_dist, &release, &existing, &manifest)
            .await?;
        ensure!(!assets.is_empty(), "cargo-dist produced no assets");
        manifest.clear_paths();
        let receipt = Receipt {
            schema: RECEIPT_SCHEMA,
            job,
            tag: self.tag.clone(),
            commit: self.commit.clone(),
            manifest,
            assets,
        };
        self.client
            .dist_upload(
                &release,
                &existing,
                &receipt.name(),
                serde_json::to_vec(&receipt)?,
            )
            .await?;
        Ok(())
    }

    /// Run after the matrix succeeds. Refuse to publish unless every matrix slot has a receipt.
    pub async fn finalize(&self, run_id: &str) -> anyhow::Result<()> {
        validate_run_id(run_id)?;
        let release = self.client.dist_release(&self.tag).await?;
        if !release.draft {
            tracing::info!("Release {} is already published", self.tag);
            return Ok(());
        }
        let assets = self.client.dist_assets(&release).await?;
        let prefix = receipt_prefix(run_id);
        let mut receipts = vec![];
        for asset in &assets {
            if asset.name.starts_with(&prefix) && asset.name.ends_with(".json") {
                ensure!(
                    asset.size <= 16 * 1024 * 1024,
                    "distribution receipt is too large"
                );
                let receipt: Receipt =
                    serde_json::from_slice(&self.client.dist_download(asset).await?)
                        .context("invalid distribution receipt")?;
                ensure!(
                    receipt.name() == asset.name,
                    "distribution receipt name mismatch"
                );
                receipts.push(receipt);
            }
        }
        let targets = validate_receipts(
            &receipts,
            run_id,
            &self.tag,
            &self.commit,
            &self.package,
            &assets,
        )?;
        let cargo_dist = self.cargo_dist(targets)?;
        for (i, receipt) in receipts.iter().enumerate() {
            cargo_dist.import_manifest(i, &receipt.manifest)?;
        }
        let mut manifest = self.build_manifest(&cargo_dist, true)?;
        self.upload_manifest_artifacts(&cargo_dist, &release, &assets, &manifest)
            .await?;
        let notes = manifest.installation_notes()?;
        ensure!(
            !notes.trim().is_empty(),
            "cargo-dist generated empty release notes"
        );
        let body = body_with_installation_notes(release.body.as_deref().unwrap_or_default(), notes);
        manifest.clear_paths();
        self.client
            .dist_upload(
                &release,
                &assets,
                "dist-manifest.json",
                serde_json::to_vec(&manifest)?,
            )
            .await?;
        // Updating the release is the final write: any earlier failure leaves a recoverable draft.
        self.client
            .dist_update_release(&release, &body, !self.draft, self.latest)
            .await?;
        if self.draft {
            tracing::info!(
                "Release {} was left as a draft because `git_release_draft` is enabled",
                self.tag
            );
        }
        Ok(())
    }

    fn cargo_dist(&self, targets: Vec<String>) -> anyhow::Result<CargoDist> {
        CargoDist::prepare(
            &self.project,
            self.request.cargo_metadata(),
            &self.package,
            &self.repository,
            &self.tag,
            targets,
        )
    }

    /// Run cargo-dist and check that it built exactly this package and tag.
    fn build_manifest(&self, cargo_dist: &CargoDist, global: bool) -> anyhow::Result<Manifest> {
        let manifest = cargo_dist.build(global)?;
        manifest.validate(&self.tag, &self.package)?;
        Ok(manifest)
    }

    /// Fetch the release for the tag and refuse to continue unless it is still a draft.
    async fn require_draft_release(&self) -> anyhow::Result<GitHubRelease> {
        let release = self.client.dist_release(&self.tag).await?;
        ensure!(release.draft, "release `{}` is already published", self.tag);
        Ok(release)
    }

    async fn upload_manifest_artifacts(
        &self,
        cargo_dist: &CargoDist,
        release: &GitHubRelease,
        existing: &[Asset],
        manifest: &Manifest,
    ) -> anyhow::Result<Vec<Asset>> {
        let mut assets = vec![];
        for artifact in manifest.artifacts.values() {
            if let Some(path) = &artifact.path {
                let name = artifact
                    .name
                    .as_deref()
                    .context("cargo-dist artifact has no name")?;
                ensure!(
                    path.file_name().and_then(|n| n.to_str()) == Some(name),
                    "cargo-dist artifact name mismatch"
                );
                assets.push(
                    self.client
                        .dist_upload(release, existing, name, cargo_dist.artifact_bytes(path)?)
                        .await?,
                );
            }
        }
        Ok(assets)
    }
}

/// Matrix identity supplied automatically by the distribution action.
#[derive(Debug, Serialize, Deserialize)]
pub struct DistJob {
    pub run_id: String,
    pub index: usize,
    pub total: usize,
    pub target: String,
}

impl DistJob {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_matrix(
            run_id()?,
            &std::env::var("RELEASE_PLZ_DIST_JOB_INDEX").unwrap_or_default(),
            &std::env::var("RELEASE_PLZ_DIST_JOB_TOTAL").unwrap_or_default(),
            &std::env::var("RELEASE_PLZ_DIST_MATRIX").unwrap_or_else(|_| "null".into()),
        )
    }

    /// Identify this job from the matrix `index`/`total` and the JSON `matrix` context.
    /// Without a matrix, the job is the only one and builds for the host target.
    fn from_matrix(run_id: String, index: &str, total: &str, matrix: &str) -> anyhow::Result<Self> {
        let slot = match (parse_optional_usize(index)?, parse_optional_usize(total)?) {
            (Some(index), Some(total)) => Some((index, total)),
            (None, None) => None,
            _ => anyhow::bail!("both matrix job index and total must be provided"),
        };
        let matrix: Value =
            serde_json::from_str(matrix).context("invalid distribution matrix context")?;
        ensure!(
            matrix.is_null() || slot.is_some(),
            "matrix context requires a job index and total"
        );
        let target = match matrix.get("target") {
            None | Some(Value::Null) => cargo_dist::host_target()?,
            Some(Value::String(target)) => target.clone(),
            Some(_) => anyhow::bail!("matrix.target must be a Rust target triple string"),
        };
        let (index, total) = slot.unwrap_or((0, 1));
        let job = Self {
            run_id,
            index,
            total,
            target,
        };
        job.validate()?;
        Ok(job)
    }

    fn validate(&self) -> anyhow::Result<()> {
        validate_run_id(&self.run_id)?;
        ensure!(
            self.total > 0 && self.total <= 256 && self.index < self.total,
            "invalid distribution matrix identity"
        );
        ensure!(
            !self.target.is_empty()
                && self
                    .target
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.'),
            "invalid Rust target triple"
        );
        Ok(())
    }
}

pub fn run_id() -> anyhow::Result<String> {
    let run_id = std::env::var("GITHUB_RUN_ID")
        .context("distribution commands must run in GitHub Actions (GITHUB_RUN_ID is missing)")?;
    validate_run_id(&run_id)?;
    Ok(run_id)
}

fn parse_optional_usize(value: &str) -> anyhow::Result<Option<usize>> {
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse()
        .map(Some)
        .with_context(|| format!("invalid matrix job number `{value}`"))
}

fn validate_run_id(run_id: &str) -> anyhow::Result<()> {
    ensure!(
        !run_id.is_empty() && run_id.bytes().all(|b| b.is_ascii_digit()),
        "invalid GitHub run ID"
    );
    Ok(())
}

const RECEIPT_SCHEMA: u32 = 1;

fn receipt_prefix(run_id: &str) -> String {
    format!("release-plz-dist-{run_id}-")
}

#[derive(Debug, Serialize, Deserialize)]
struct Receipt {
    schema: u32,
    job: DistJob,
    tag: String,
    commit: String,
    manifest: Manifest,
    assets: Vec<Asset>,
}

impl Receipt {
    fn name(&self) -> String {
        format!(
            "{}{}.json",
            receipt_prefix(&self.job.run_id),
            self.job.index
        )
    }
}

// Keep cargo-dist's unknown fields intact: they carry linkage and platform data used by installers.
#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    dist_version: String,
    announcement_tag: String,
    announcement_github_body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    announcement_changelog: Option<String>,
    releases: Vec<ManifestRelease>,
    artifacts: BTreeMap<String, Artifact>,
    /// Local paths cargo-dist would upload itself; dropped from receipts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    upload_files: Option<Value>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

impl Manifest {
    /// Whether cargo-dist built an executable archive for exactly `target`.
    fn has_binary_for(&self, target: &str) -> bool {
        self.artifacts
            .values()
            .any(|a| a.kind == "executable-zip" && a.target_triples == [target])
    }

    fn installation_notes(&self) -> anyhow::Result<&str> {
        let body = self
            .announcement_github_body
            .as_deref()
            .context("cargo-dist did not generate release notes")?;
        // Release-plz owns the changelog. Strip only cargo-dist's known changelog
        // prefix; retain its generated installer instructions and download table.
        if let Some(changelog) = &self.announcement_changelog {
            body.strip_prefix(&format!("## Release Notes\n\n{changelog}\n\n"))
                .context("unexpected cargo-dist release notes format")
        } else {
            Ok(body)
        }
    }

    fn validate(&self, tag: &str, package: &Package) -> anyhow::Result<()> {
        ensure!(
            self.dist_version == cargo_dist::VERSION,
            "unexpected cargo-dist version"
        );
        ensure!(
            self.announcement_tag == tag,
            "cargo-dist tag does not match release"
        );
        ensure!(
            self.releases.len() == 1
                && self.releases[0].app_name == package.name.as_str()
                && self.releases[0].app_version == package.version.to_string(),
            "cargo-dist must select exactly the requested package and version"
        );
        Ok(())
    }

    fn clear_paths(&mut self) {
        for artifact in self.artifacts.values_mut() {
            artifact.path = None;
        }
        self.upload_files = None;
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ManifestRelease {
    app_name: String,
    app_version: String,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Artifact {
    name: Option<String>,
    kind: String,
    #[serde(default)]
    target_triples: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<PathBuf>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

fn validate_receipts(
    receipts: &[Receipt],
    run_id: &str,
    tag: &str,
    commit: &str,
    package: &Package,
    assets: &[Asset],
) -> anyhow::Result<Vec<String>> {
    let first = receipts
        .first()
        .context("no successful distribution builds for this workflow run")?;
    ensure!(
        receipts.len() == first.job.total,
        "distribution matrix is incomplete: got {} of {} builds",
        receipts.len(),
        first.job.total
    );
    let mut indices = BTreeSet::new();
    let mut targets = BTreeSet::new();
    for receipt in receipts {
        receipt.job.validate()?;
        ensure!(
            receipt.schema == RECEIPT_SCHEMA
                && receipt.job.run_id == run_id
                && receipt.tag == tag
                && receipt.commit == commit,
            "distribution receipt does not match this run, tag or commit"
        );
        ensure!(
            receipt.job.total == first.job.total && indices.insert(receipt.job.index),
            "inconsistent matrix receipts"
        );
        ensure!(
            targets.insert(receipt.job.target.clone()),
            "multiple matrix jobs built the same target"
        );
        receipt.manifest.validate(tag, package)?;
        ensure!(
            receipt.manifest.has_binary_for(&receipt.job.target) && !receipt.assets.is_empty(),
            "distribution receipt contains no binaries"
        );
        for built in &receipt.assets {
            ensure!(
                assets.iter().any(|asset| asset.id == built.id
                    && asset.name == built.name
                    && asset.size == built.size
                    && asset.state == "uploaded"),
                "release asset `{}` is missing or was replaced; rerun its build job",
                built.name
            );
        }
        for artifact in receipt.manifest.artifacts.values() {
            ensure!(
                artifact.path.is_none(),
                "distribution receipt contains local paths"
            );
            if let Some(name) = &artifact.name {
                ensure!(
                    receipt.assets.iter().any(|asset| &asset.name == name),
                    "receipt is missing artifact `{name}`"
                );
            }
        }
    }
    Ok(targets.into_iter().collect())
}

/// Delimits the installation notes appended to the release body.
const NOTES_START: &str = "<!-- release-plz-dist -->";
const NOTES_END: &str = "<!-- /release-plz-dist -->";

/// Insert `notes` into `body`, replacing the notes of an earlier finalize run so a retry
/// (for example when `git_release_draft` keeps the release a draft) is idempotent.
/// Text before and after the delimited block, such as a manually edited draft, is kept.
fn body_with_installation_notes(body: &str, notes: &str) -> String {
    let (head, tail) = match body.split_once(NOTES_START) {
        Some((head, rest)) => (
            head,
            rest.split_once(NOTES_END).map_or("", |(_, tail)| tail),
        ),
        None => (body, ""),
    };
    let notes = format!("{NOTES_START}\n{}\n{NOTES_END}", notes.trim());
    let mut body = [head.trim_end(), &notes, tail.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    body.push('\n');
    body
}
