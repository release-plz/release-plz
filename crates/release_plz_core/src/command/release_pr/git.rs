use anyhow::{Context, Result, bail};
use cargo_metadata::semver::Version;
use git2::{Oid, Repository, Worktree, WorktreePruneOptions};
use regex::Regex;
use std::path::Path;
use tempfile::TempDir;
use tracing::{debug, error, info, instrument, warn};

use crate::fs_utils::to_utf8_path;

pub struct CustomRepo {
    repo: Repository,
}

impl CustomRepo {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        info!("Opening git repo at {:?}", path.as_ref());
        let repo = Repository::open(path).context("open repository")?;
        Ok(Self { repo })
    }

    pub fn temp_worktree(
        &mut self,
        path_suffix: Option<&str>,
        name: &str,
    ) -> Result<CustomWorkTree> {
        // Use tempfile::Builder to generate a unique path (prevents timestamp collisions)
        let prefix = if let Some(suffix) = path_suffix {
            format!("release-plz-{}-", suffix)
        } else {
            "release-plz-worktree-".to_string()
        };

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

        info!("Creating worktree called {name} at {path}");
        let wt = self
            .repo
            .worktree(name, path_std, None)
            .context("create worktree")?;
        Ok(CustomWorkTree {
            worktree: wt,
            _tmp_dir_handle: temp_dir,
        })
    }

    pub fn get_tags(&self) -> Result<Vec<String>> {
        let tags: Vec<String> = self
            .repo
            .tag_names(None)
            .context("get tags for repo")?
            .iter()
            .filter_map(|x| x.map(|s| s.to_string()))
            .collect();
        Ok(tags)
    }

    /// Get the commit and version from within the tag message
    /// NOTE: This version isn't actually used for anything, we extract the package version from
    /// the Cargo.toml for packages, so if tag "v0.1.5" points to a commit where the Cargo.toml
    /// within that tree that has version 0.1.4, we use 0.1.4 for the package version
    #[instrument(skip(release_tag_regex, self))]
    pub fn get_release_tag(
        &self,
        release_tag_regex: &Regex,
        package_name: &str,
    ) -> Result<Option<(String, Version)>> {
        // get the tags for this repo
        let tags = self.get_tags().context("get tags for package")?;
        debug!("Found {} total tags: {:?}", tags.len(), tags);

        // Find the most recent release tags
        let tag_results: Vec<(String, Result<Version, _>)> = tags
            .iter()
            .filter_map(|tag| {
                release_tag_regex.captures(tag).map(|captures| {
                    let version_str = captures
                        .get(1)
                        .expect("capture group 1 must exist in our regex")
                        .as_str();
                    debug!(
                        "Tag `{}` matches pattern, version string: {}",
                        tag, version_str
                    );
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
                    warn!(
                        "Tag `{}` matched pattern but failed to parse version: {}",
                        tag, e
                    );
                }
            }
        }

        // Sort by version (descending) and take the highest
        // NOTE: I wasn't completely sure whether we wanted the latest tag, or the highest. I
        // opted for the highest since it was less work and both of them seem reasonable.
        release_tags.sort_by(|a, b| b.1.cmp(&a.1));

        Ok(release_tags.get(0).cloned())
    }

    /// Get the commit associated with the tag (either an annotated tag, or a lightweight tag)
    /// We purposefully dont return option here because a tag MUST be associated with a commit,
    /// either through reference in an annotated tag object or just pointing to a commit
    /// (lightweight)
    pub fn get_tag_commit(&self, tag_name: &str) -> Result<String> {
        // First, try to find as an annotated tag
        let mut commit: Option<Oid> = None;
        self.repo
            .tag_foreach(|oid, _| {
                // Skip if we already found the tag
                if commit.is_some() {
                    return true;
                }

                let tag = match self.repo.find_tag(oid) {
                    Ok(t) => t,
                    Err(_) => {
                        // Not an annotated tag, skip
                        return true;
                    }
                };

                let tag_name_from_repo = match tag.name() {
                    Some(name) => name,
                    None => {
                        warn!("Annotated tag has no name, skipping");
                        return true;
                    }
                };

                if tag_name_from_repo == tag_name {
                    let target = match tag.target() {
                        Ok(obj) => obj,
                        Err(e) => {
                            error!("Error getting target for annotated tag {}: {e:?}", tag_name);
                            return true;
                        }
                    };

                    let commit_obj = match target.into_commit() {
                        Ok(c) => c,
                        Err(obj) => {
                            error!(
                                "Annotated tag {} does not point to a commit, points to {:?}",
                                tag_name,
                                obj.kind()
                            );
                            return true;
                        }
                    };

                    commit = Some(commit_obj.id());
                }

                true
            })
            .context("failed to iterate over tags")?;

        if let Some(id) = commit {
            info!("Found annotated tag '{}'", tag_name);
            return Ok(id.to_string());
        }

        // If not found as annotated tag, try as lightweight tag
        let tag_ref_name = format!("refs/tags/{}", tag_name);
        match self.repo.find_reference(&tag_ref_name) {
            Ok(reference) => {
                // Resolve the reference to get the commit it points to
                let target_id = reference.target().ok_or_else(|| {
                    anyhow::anyhow!("Lightweight tag '{}' has no target", tag_name)
                })?;

                // Verify it points to a commit
                let object = self.repo.find_object(target_id, None).with_context(|| {
                    format!("Failed to find object for lightweight tag '{}'", tag_name)
                })?;

                let commit = object.peel_to_commit().with_context(|| {
                    format!("Lightweight tag '{}' does not point to a commit", tag_name)
                })?;

                info!("Found lightweight tag '{}'", tag_name);
                Ok(commit.id().to_string())
            }
            Err(_) => {
                bail!(
                    "No tag found with name '{}'. \
                    Please create a tag with 'git tag {}' (lightweight) or 'git tag -a {}' (annotated).",
                    tag_name,
                    tag_name,
                    tag_name
                )
            }
        }
    }

    /// Checkout a particular commit
    pub fn checkout_commit(&mut self, commit_sha: &str) -> Result<()> {
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

        info!("Checked out commit {commit_sha}");
        Ok(())
    }

    /// Delete a local branch
    pub fn delete_branch(&mut self, branch_name: &str) -> Result<()> {
        let mut branch = self
            .repo
            .find_branch(branch_name, git2::BranchType::Local)
            .context("find local branch")?;
        branch.delete().context("delete branch")?;
        Ok(())
    }

    /// Clean up existing worktree and its branch if they exist
    pub fn cleanup_worktree_if_exists(&mut self, name: &str) -> Result<()> {
        let trees: Vec<String> = self
            .repo
            .worktrees()
            .context("get worktrees for repo")?
            .iter()
            .filter_map(|x| x.map(|s| s.to_string()))
            .collect();

        if trees.contains(&name.to_string()) {
            info!("Worktree {name} already exists, cleaning it up");

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
/// https://doc.rust-lang.org/reference/destructors.html
pub struct CustomWorkTree {
    worktree: Worktree,
    _tmp_dir_handle: TempDir,
}

// NOTE: We attempt to clean up resources on drop just so we don't forget to do so. Failing to
// clean the resources just logs things, although if things fail it may cause problems the next
// time you run (but shouldn't because we use random temp dirs anyways)
impl Drop for CustomWorkTree {
    fn drop(&mut self) {
        info!(
            "cleaning up worktree {:?}",
            self.path().to_str().unwrap_or_default()
        );

        // open a repo so we can delete the branch, would be better to hold a reference back to the
        // original repo to do it for us (similar to RefCell), but this will suffice for now
        let mut repo = match CustomRepo::open(self.path()) {
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
        match repo.delete_branch(self.worktree.name().unwrap_or_default()) {
            Ok(_) => {}
            Err(e) => error!("Error deleting branch: {e:?}"),
        };

        // death to the trees!
        if let Err(e) = self.worktree.prune(Some(
            WorktreePruneOptions::new().working_tree(true).valid(true),
        )) {
            warn!("Couldn't prune worktree: {e:?}");
        }
    }
}

impl CustomWorkTree {
    pub fn path(&self) -> &Path {
        self.worktree.path()
    }
}
