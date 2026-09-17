use std::path::Path;

use super::*;

/// Refine equality-based ancestry pruning with the changes still present at HEAD.
///
/// A merge can keep a change whose descendant on another branch matches the
/// release. Follow Git's simplified parent graph, stopping each lineage at equal
/// snapshots. For ancestors reachable through another lineage, revert the change
/// in memory to distinguish surviving contributions from discarded merge parents.
/// A revert that leaves the release unchanged proves a change was absent there.
/// Sequential edits can make that revert conflict; an earlier equal snapshot can
/// still establish the start of the sequence, provided this commit can be undone
/// cleanly at HEAD.
pub(super) struct RetainedChanges {
    repo: git2::Repository,
    head: git2::Oid,
    released: git2::Oid,
    package_files: Option<HashSet<Utf8PathBuf>>,
    paths: Vec<Utf8PathBuf>,
    parents: HashMap<String, Vec<String>>,
    root: Option<String>,
    boundaries: HashSet<String>,
    reachable: HashSet<String>,
}

enum Contribution {
    Absent,
    Present,
    Conflict,
}

impl RetainedChanges {
    pub(super) fn new(
        repository: &Repo,
        head: &str,
        released: &str,
        package_files: Option<HashSet<Utf8PathBuf>>,
        paths: &[Utf8PathBuf],
    ) -> anyhow::Result<Self> {
        let mut args = vec!["rev-list", "--parents", "--date-order", head, "--"];
        args.extend(paths.iter().map(|path| path.as_str()));
        let graph = repository.git(&args)?;
        let root = graph.split_whitespace().next().map(str::to_owned);
        let parents = graph
            .lines()
            .filter_map(|line| {
                let mut ids = line.split_whitespace();
                Some((ids.next()?.to_owned(), ids.map(str::to_owned).collect()))
            })
            .collect();
        Ok(Self {
            repo: git2::Repository::open(repository.directory())?,
            head: git2::Oid::from_str(head)?,
            released: git2::Oid::from_str(released)?,
            package_files,
            paths: paths
                .iter()
                .map(|path| {
                    path.strip_prefix(repository.directory())
                        .map(Utf8Path::to_path_buf)
                })
                .collect::<Result<_, _>>()?,
            parents,
            root,
            boundaries: HashSet::new(),
            reachable: HashSet::new(),
        })
    }

    pub(super) fn add_boundary(&mut self, commit: &str) {
        self.boundaries.insert(commit.to_owned());
        self.reachable.clear();
        let mut pending: Vec<_> = self.root.iter().map(String::as_str).collect();
        while let Some(commit) = pending.pop() {
            if self.boundaries.contains(commit) || !self.reachable.insert(commit.to_owned()) {
                continue;
            }
            if let Some(parents) = self.parents.get(commit) {
                pending.extend(parents.iter().map(String::as_str));
            }
        }
    }

    pub(super) fn contains(&self, commit: &str) -> bool {
        if !self.reaches(commit) {
            return false;
        }
        let contribution = self.check(commit).unwrap_or_else(|error| {
            // Shallow histories may not contain the parent required for a revert.
            // In that case there is no evidence to override ancestry pruning.
            debug!("cannot check retained changes in {commit}: {error:#}");
            Contribution::Absent
        });
        match contribution {
            Contribution::Absent => false,
            // A sibling editing the same lines as a reverted commit can make its
            // undo conflict. The reachability check above ensures another lineage
            // reaches it without passing an equal package snapshot.
            Contribution::Present | Contribution::Conflict => true,
        }
    }

    pub(super) fn reaches(&self, commit: &str) -> bool {
        self.reachable.contains(commit)
    }

    pub(super) fn descends_from(&self, commit: &str, ancestor: &str) -> bool {
        git2::Oid::from_str(commit)
            .and_then(|id| {
                self.repo
                    .graph_descendant_of(id, git2::Oid::from_str(ancestor)?)
            })
            .unwrap_or(false)
    }

    fn check(&self, commit_id: &str) -> anyhow::Result<Contribution> {
        let commit = self.repo.find_commit(git2::Oid::from_str(commit_id)?)?;
        let released = self.repo.find_commit(self.released)?;
        let head = self.repo.find_commit(self.head)?;
        match self.undo_changes_package(&commit, &released)? {
            // Once absence from the release is established, a conflict at HEAD
            // is ambiguous: preserve the breaking-change marker.
            Contribution::Absent => self.undo_changes_package(&commit, &head),
            Contribution::Present => Ok(Contribution::Absent),
            Contribution::Conflict => {
                // The commit may depend on an earlier edit to the same lines.
                // An equal ancestor bounds that sequence just like a linear
                // unreleased history. Require a clean undo of this individual
                // commit at HEAD: surviving earlier edits alone are insufficient.
                if matches!(
                    self.undo_changes_package(&commit, &head)?,
                    Contribution::Present
                ) && self
                    .boundaries
                    .iter()
                    .any(|boundary| self.descends_from(commit_id, boundary))
                {
                    return Ok(Contribution::Present);
                }
                Ok(Contribution::Absent)
            }
        }
    }

    fn undo_changes_package(
        &self,
        commit: &git2::Commit<'_>,
        target: &git2::Commit<'_>,
    ) -> anyhow::Result<Contribution> {
        // For merge commits, undo the change relative to the first parent, as
        // `git revert -m 1` does. Root commits are handled by libgit2's empty base.
        let mainline = u32::from(commit.parent_count() > 1);
        let index = self.repo.revert_commit(commit, target, mainline, None)?;
        for conflict in index.conflicts()? {
            let conflict = conflict?;
            let changes_file_presence = conflict.our.as_ref().map(|entry| &entry.path)
                != conflict.their.as_ref().map(|entry| &entry.path);
            for entry in [conflict.ancestor, conflict.our, conflict.their]
                .into_iter()
                .flatten()
            {
                if std::str::from_utf8(&entry.path)
                    .map(|path| self.includes(Path::new(path), changes_file_presence))
                    .unwrap_or(true)
                {
                    return Ok(Contribution::Conflict);
                }
            }
        }
        let tree = target.tree()?;
        let diff = self
            .repo
            .diff_tree_to_index(Some(&tree), Some(&index), None)?;
        let changed = diff.deltas().any(|delta| {
            let changes_file_presence =
                matches!(delta.status(), git2::Delta::Added | git2::Delta::Deleted);
            [delta.old_file().path(), delta.new_file().path()]
                .into_iter()
                .flatten()
                .any(|path| self.includes(path, changes_file_presence))
        });
        Ok(if changed {
            Contribution::Present
        } else {
            Contribution::Absent
        })
    }

    fn includes(&self, path: &Path, changes_file_presence: bool) -> bool {
        let Some(path) = Utf8Path::from_path(path) else {
            return true;
        };
        // Match package equality: generated files are ignored at the package root.
        let package_relative_path = self
            .paths
            .first()
            .and_then(|root| path.strip_prefix(root).ok());
        if matches!(
            package_relative_path.map(Utf8Path::as_str),
            Some("Cargo.lock" | CARGO_TOML_ORIG | CARGO_VCS_INFO)
        ) {
            return false;
        }
        // Nested lockfiles and original manifests contribute to the package file
        // list even though their contents are excluded from equality checks.
        if !changes_file_presence
            && matches!(path.file_name(), Some("Cargo.lock" | CARGO_TOML_ORIG))
        {
            return false;
        }
        if let Some(files) = &self.package_files {
            files.contains(path)
                // An overridden README can live outside the package directory.
                || self.paths.iter().skip(1).any(|readme| path == readme)
        } else {
            // File listing failures already make package commit filtering
            // conservative; retain that behavior for the content check as well.
            self.paths.iter().any(|root| path.starts_with(root))
        }
    }
}
