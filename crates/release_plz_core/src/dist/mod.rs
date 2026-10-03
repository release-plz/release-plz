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
use cargo_metadata::{Metadata, Package};
use git_cmd::Repo;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ForgeType, GitClient, Project, ReleaseRequest};
use cargo_dist::CargoDist;
use github::{Asset, Release};

/// A distribution is always tied to one package, tag and checked-out commit.
#[derive(Debug)]
pub struct DistRequest {
    metadata: Metadata,
    package: Package,
    project: Project,
    tag: String,
    commit: String,
    client: GitClient,
    repository: String,
    latest: Option<bool>,
}

impl DistRequest {
    pub fn new(
        metadata: Metadata,
        config: &ReleaseRequest,
        tag: String,
        client: GitClient,
        repository: String,
    ) -> anyhow::Result<Self> {
        ensure!(client.forge == ForgeType::Github, "dist requires GitHub");
        let project = Project::new(
            &metadata.workspace_root.join("Cargo.toml"),
            None,
            &HashSet::new(),
            &metadata,
            config,
        )?;
        let mut packages = vec![];
        for package in project.workspace_packages() {
            if metadata.workspace_members.contains(&package.id)
                && config.get_package_config(&package.name).dist()
                && project.git_tag(&package.name, &package.version.to_string())? == tag
            {
                packages.push(package);
            }
        }
        ensure!(
            packages.len() == 1,
            "tag `{tag}` must select exactly one package with dist=true"
        );
        let package = packages[0].clone();
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
        let latest = config
            .get_package_config(&package.name)
            .git_release()
            .latest();
        Ok(Self {
            metadata,
            package,
            project,
            tag,
            commit,
            client,
            repository,
            latest,
        })
    }

    /// Build for this runner and upload assets, then a receipt indicating successful completion.
    pub async fn build(&self, job: DistJob) -> anyhow::Result<()> {
        job.validate()?;
        let release = self.client.dist_release(&self.tag).await?;
        ensure!(release.draft, "release `{}` is already published", self.tag);
        let targets = vec![job.target.clone()];
        let cargo_dist = CargoDist::prepare(
            &self.project,
            &self.metadata,
            &self.package,
            &self.repository,
            &targets,
        )?;
        let mut manifest = cargo_dist.build(&self.tag, &targets, false)?;
        manifest.validate(&self.tag, &self.package)?;
        ensure!(
            manifest
                .artifacts
                .values()
                .any(|a| a.kind == "executable-zip" && a.target_triples == targets),
            "cargo-dist did not build binaries for {}",
            job.target
        );
        let assets = self
            .upload_manifest_artifacts(&cargo_dist, &release, &manifest)
            .await?;
        ensure!(!assets.is_empty(), "cargo-dist produced no assets");
        manifest.clear_paths();
        let receipt = Receipt {
            schema: 1,
            job,
            tag: self.tag.clone(),
            commit: self.commit.clone(),
            manifest,
            assets,
        };
        self.client
            .dist_upload(&release, &receipt.name(), serde_json::to_vec(&receipt)?)
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
        let prefix = format!("release-plz-dist-{run_id}-");
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
        let cargo_dist = CargoDist::prepare(
            &self.project,
            &self.metadata,
            &self.package,
            &self.repository,
            &targets,
        )?;
        for (i, receipt) in receipts.iter().enumerate() {
            cargo_dist.import_manifest(i, &receipt.manifest)?;
        }
        let mut manifest = cargo_dist.build(&self.tag, &targets, true)?;
        manifest.validate(&self.tag, &self.package)?;
        self.upload_manifest_artifacts(&cargo_dist, &release, &manifest)
            .await?;
        let notes = manifest.installation_notes()?;
        ensure!(
            !notes.trim().is_empty(),
            "cargo-dist generated empty release notes"
        );
        let body = release_body(release.body.as_deref().unwrap_or_default(), notes);
        manifest.clear_paths();
        self.client
            .dist_upload(
                &release,
                "dist-manifest.json",
                serde_json::to_vec(&manifest)?,
            )
            .await?;
        // Publishing is the final write: any earlier failure leaves a recoverable draft.
        self.client
            .dist_publish(&release, &body, self.latest)
            .await?;
        Ok(())
    }

    async fn upload_manifest_artifacts(
        &self,
        cargo_dist: &CargoDist,
        release: &Release,
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
                        .dist_upload(release, name, cargo_dist.artifact_bytes(path)?)
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
        let run_id = run_id()?;
        let index = std::env::var("RELEASE_PLZ_DIST_JOB_INDEX").unwrap_or_default();
        let total = std::env::var("RELEASE_PLZ_DIST_JOB_TOTAL").unwrap_or_default();
        ensure!(
            index.is_empty() == total.is_empty(),
            "both matrix job index and total must be provided"
        );
        let matrix: Value = serde_json::from_str(
            &std::env::var("RELEASE_PLZ_DIST_MATRIX").unwrap_or_else(|_| "null".into()),
        )?;
        ensure!(
            matrix.is_null() || !total.is_empty(),
            "matrix context requires a job index and total"
        );
        let target = match matrix.get("target") {
            None | Some(Value::Null) => cargo_dist::host_target()?,
            Some(Value::String(target)) => target.clone(),
            Some(_) => anyhow::bail!("matrix.target must be a Rust target triple string"),
        };
        let job = Self {
            run_id,
            index: if index.is_empty() { 0 } else { index.parse()? },
            total: if total.is_empty() { 1 } else { total.parse()? },
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

fn validate_run_id(run_id: &str) -> anyhow::Result<()> {
    ensure!(
        !run_id.is_empty() && run_id.bytes().all(|b| b.is_ascii_digit()),
        "invalid GitHub run ID"
    );
    Ok(())
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
            "release-plz-dist-{}-{}.json",
            self.job.run_id, self.job.index
        )
    }
}

// Keep cargo-dist's unknown fields intact: they carry linkage and platform data used by installers.
#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    dist_version: String,
    announcement_tag: String,
    announcement_github_body: Option<String>,
    releases: Vec<ManifestRelease>,
    artifacts: BTreeMap<String, Artifact>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

impl Manifest {
    fn installation_notes(&self) -> anyhow::Result<&str> {
        let body = self
            .announcement_github_body
            .as_deref()
            .context("cargo-dist did not generate release notes")?;
        // Release-plz owns the changelog. Strip only cargo-dist's known changelog
        // prefix; retain its generated installer instructions and download table.
        if let Some(changelog) = self
            .extra
            .get("announcement_changelog")
            .and_then(Value::as_str)
        {
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
        self.extra.remove("upload_files");
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
            receipt.schema == 1
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
        let binary = receipt.manifest.artifacts.values().any(|a| {
            a.kind == "executable-zip" && a.target_triples == [receipt.job.target.clone()]
        });
        ensure!(
            binary && !receipt.assets.is_empty(),
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

fn release_body(changelog: &str, notes: &str) -> String {
    format!(
        "{}\n\n<!-- release-plz-dist -->\n{}\n<!-- /release-plz-dist -->\n",
        changelog.trim_end(),
        notes.trim()
    )
}
