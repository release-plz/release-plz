use anyhow::Context;
use cargo_metadata::semver::Version;
use git2::{Oid, Repository, Worktree, WorktreePruneOptions};
use regex::Regex;
use std::path::Path;
use tempfile::TempDir;
use tracing::{debug, error, instrument, warn};

use crate::fs_utils::to_utf8_path;

pub struct GitRepo {
    repo: Repository,
}

impl GitRepo {
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path_ref = path.as_ref();
        debug!("Opening git repo at {path_ref:?}");
        let repo = Repository::open(path_ref)
            .with_context(|| format!("failed to open repository at {}", path_ref.display()))?;
        Ok(Self { repo })
    }

    /// Create a worktree at a random path in a temporary directory
    pub fn temp_worktree(
        &mut self,
        path_suffix: Option<&str>,
        name: &str,
    ) -> anyhow::Result<GitWorkTree> {
        // Use tempfile::Builder to generate a unique path (prevents timestamp collisions)
        let suffix = path_suffix.unwrap_or("worktree");
        let prefix = format!("release-plz-{suffix}-");

        let temp_dir = tempfile::Builder::new()
            .prefix(&prefix)
            .tempdir()
            .context("create temporary directory for worktree")?;

        // Append "worktree" to get a path that doesn't exist yet (git worktree will create it)
        let temp_base = to_utf8_path(temp_dir.path())?;
        let path = temp_base.join("worktree");
        let path_std = path.as_std_path();

        // Clean up existing worktree if it exists
        self.cleanup_worktree_if_exists(name)?;

        debug!("Creating worktree called {name} at {path}");
        let wt = self
            .repo
            .worktree(name, path_std, None)
            .with_context(|| format!("create worktree at {path}"))?;
        Ok(GitWorkTree {
            worktree: wt,
            _tmp_dir_handle: temp_dir,
        })
    }

    pub fn get_tags(&self) -> anyhow::Result<Vec<String>> {
        let tags: Vec<String> = self
            .repo
            .tag_names(None)
            .context("get tags for repo")?
            .iter()
            .filter_map(|x| x.map(ToString::to_string))
            .collect();
        Ok(tags)
    }

    /// Get the commit and version from within the tag message.
    /// NOTE: This version isn't actually used for anything, we extract the package version from
    /// the Cargo.toml for packages, so if tag "v0.1.5" points to a commit where the Cargo.toml
    /// within that tree that has version 0.1.4, we use 0.1.4 for the package version
    #[instrument(skip(release_tag_regex, self))]
    pub fn get_release_tag(
        &self,
        release_tag_regex: &Regex,
        package_name: &str,
    ) -> anyhow::Result<Option<(String, Version)>> {
        // get the tags for this repo
        let tags = self
            .get_tags()
            .with_context(|| format!("get tags for package {package_name}"))?;
        debug!("Found {} total tags: {tags:?}", tags.len());

        // Find the most recent release tags
        let tag_results: Vec<(String, Result<Version, _>)> = tags
            .iter()
            .filter_map(|tag| {
                release_tag_regex.captures(tag).map(|captures| {
                    let version_str = captures
                        .get(1)
                        .expect("capture group 1 must exist in our regex")
                        .as_str();
                    debug!("Tag `{tag}` matches pattern, version string: {version_str}");
                    (tag.clone(), Version::parse(version_str))
                })
            })
            .collect();
        debug!("{} tags matched pattern", tag_results.len());

        // Separate valid and invalid tags, logging any parsing errors
        let mut release_tags: Vec<(String, Version)> = Vec::new();
        for (tag, version_result) in tag_results {
            match version_result {
                Ok(version) => release_tags.push((tag, version)),
                Err(e) => {
                    warn!("Tag `{tag}` matched pattern but failed to parse version: {e}");
                }
            }
        }

        // Sort by version (descending) and take the highest.
        // Another possible criteria is getting the latest tag, but we mimic the sorting logic of a
        // cargo registry.
        release_tags.sort_by(|a, b| b.1.cmp(&a.1));

        Ok(release_tags.into_iter().next())
    }

    /// Get the commit associated with the tag (either an annotated tag, or a lightweight tag)
    /// We purposefully don't return option here because a tag MUST be associated with a commit,
    /// either through reference in an annotated tag object or just pointing to a commit
    /// (lightweight)
    pub fn get_tag_commit(&self, tag_name: &str) -> anyhow::Result<String> {
        let tag_ref_name = format!("refs/tags/{tag_name}");

        let reference = self.repo.find_reference(&tag_ref_name).with_context(|| {
            format!(
                "No tag found with name '{tag_name}'. \
                Please create a tag with 'git tag {tag_name}' (lightweight) \
                or 'git tag -a {tag_name}' (annotated)."
            )
        })?;

        let object = reference
            .peel(git2::ObjectType::Commit)
            .with_context(|| format!("tag '{tag_name}' does not point to a commit"))?;

        let commit_id = object.id();
        debug!("Found tag '{tag_name}' pointing to {commit_id}");

        Ok(commit_id.to_string())
    }

    /// Checkout a particular commit
    pub fn checkout_commit(&mut self, commit_sha: &str) -> anyhow::Result<()> {
        // first we convert the string to an Oid
        let id = Oid::from_str(commit_sha).context("convert commit sha to oid")?;

        // Actually checkout the files to update the working directory
        let obj = self.repo.find_object(id, None).context("find object")?;
        let mut checkout_builder = git2::build::CheckoutBuilder::new();
        checkout_builder.force(); // Force checkout to overwrite working directory
        self.repo
            .checkout_tree(&obj, Some(&mut checkout_builder))
            .context("checkout tree to update working directory")?;

        // Set HEAD after checkout
        self.repo
            .set_head_detached(id)
            .context("set head to detached commit")?;

        debug!("Checked out commit {commit_sha}");
        Ok(())
    }

    /// Delete a local branch
    pub fn delete_branch(&mut self, branch_name: &str) -> anyhow::Result<()> {
        let mut branch = self
            .repo
            .find_branch(branch_name, git2::BranchType::Local)
            .context("find local branch")?;
        branch.delete().context("delete branch")?;
        Ok(())
    }

    /// Clean up existing worktree and its branch if they exist
    pub fn cleanup_worktree_if_exists(&mut self, name: &str) -> anyhow::Result<()> {
        let trees: Vec<String> = self
            .repo
            .worktrees()
            .context("get worktrees for repo")?
            .iter()
            .filter_map(|x| x.map(ToString::to_string))
            .collect();

        if trees.contains(&name.to_string()) {
            debug!("Worktree {name} already exists, cleaning it up");

            // Find the worktree
            let wt = match self.repo.find_worktree(name) {
                Ok(wt) => wt,
                Err(e) => {
                    warn!("Error finding worktree {name} for cleanup: {e:?}");
                    return Ok(());
                }
            };

            // Prune the worktree
            if let Err(e) = wt.prune(Some(
                WorktreePruneOptions::new().working_tree(true).valid(true),
            )) {
                warn!("Error pruning worktree {name}: {e:?}");
            }

            // Delete the branch
            if let Err(e) = self.delete_branch(name) {
                warn!("Error deleting branch {name}: {e:?}");
            }
        }

        Ok(())
    }
}

/// We maintain a handle to the temp dir so it doesn't delete itself before the worktree is cleaned
/// up
/// NOTE: As stated in the destructor docs: "The fields of a struct are dropped in declaration order."
/// <https://doc.rust-lang.org/reference/destructors.html>
pub struct GitWorkTree {
    worktree: Worktree,
    _tmp_dir_handle: TempDir,
}

// NOTE: We attempt to clean up resources on drop just so we don't forget to do so. Failing to
// clean the resources just logs things, although if things fail it may cause problems the next
// time you run (but shouldn't because we use random temp dirs anyways)
impl Drop for GitWorkTree {
    fn drop(&mut self) {
        debug!(
            "cleaning up worktree {:?}",
            self.path().to_str().unwrap_or_default()
        );

        // open a repo so we can delete the branch, would be better to hold a reference back to the
        // original repo to do it for us (similar to RefCell), but this will suffice for now
        let mut repo = match GitRepo::open(self.path()) {
            Ok(r) => r,
            Err(e) => {
                error!("Error creating repo to drop branch: {e:?}");
                return;
            }
        };

        // change to head so we can delete this branch
        let head_target = match repo.repo.head() {
            Ok(head) => match head.target() {
                Some(target) => target,
                None => {
                    warn!("Head has no target, cannot detach head for worktree cleanup");
                    return;
                }
            },
            Err(e) => {
                warn!("Error getting head for worktree cleanup: {e:?}");
                return;
            }
        };
        if let Err(e) = repo.repo.set_head_detached(head_target) {
            warn!("Error setting head detached for worktree cleanup: {e:?}");
            return;
        }

        // go ahead and delete the branch now
        if let Err(e) = repo.delete_branch(self.worktree.name().unwrap_or_default()) {
            error!("Error deleting branch: {e:?}");
        }

        // death to the trees!
        if let Err(e) = self.worktree.prune(Some(
            WorktreePruneOptions::new().working_tree(true).valid(true),
        )) {
            warn!("Couldn't prune worktree: {e:?}");
        }
    }
}

impl GitWorkTree {
    pub fn path(&self) -> &Path {
        self.worktree.path()
    }
}
