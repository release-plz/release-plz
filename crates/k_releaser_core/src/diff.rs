use git_cliff_core::{commit::Signature, contributor::RemoteContributor};

use crate::semver_check::SemverCheck;

/// Difference between local package and last git-tagged version
#[derive(Debug, Clone)]
pub(crate) struct Diff {
    /// List of commits from last tagged version to last local changes.
    pub commits: Vec<Commit>,
    /// Semver incompatible changes.
    pub semver_check: SemverCheck,
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct Commit {
    pub id: String,
    pub message: String,
    pub author: Signature,
    pub committer: Signature,
    pub remote: RemoteContributor,
}

impl Commit {
    pub fn new(id: String, message: String) -> Self {
        Self {
            id,
            message,
            ..Self::default()
        }
    }

    pub fn to_cliff_commit(&self) -> git_cliff_core::commit::Commit<'_> {
        let remote = self.remote.username.is_some().then(|| self.remote.clone());
        git_cliff_core::commit::Commit {
            id: self.id.clone(),
            message: self.message.clone(),
            author: self.author.clone(),
            committer: self.committer.clone(),
            remote,
            ..Default::default()
        }
    }
}

impl Diff {
    pub fn new() -> Self {
        Self {
            commits: vec![],
            semver_check: SemverCheck::Skipped,
        }
    }

    pub fn add_commits(&mut self, commits: &[Commit]) {
        for c in commits {
            if !self.commits.contains(c) {
                self.commits.push(c.clone());
            }
        }
    }
}
