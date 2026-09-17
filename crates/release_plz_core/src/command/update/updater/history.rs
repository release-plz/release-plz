use std::path::Path;

use super::*;

/// Refine equality-based ancestry pruning with the changes still present at HEAD.
///
/// A merge can keep a change whose descendant on another branch matches the
/// release. Follow Git's simplified parent graph, stopping each lineage at equal
/// snapshots. For ancestors reachable through another lineage, revert the change
/// in memory to distinguish surviving contributions from discarded merge parents.
/// First require that the same revert leaves the released snapshot unchanged:
/// otherwise its absence from the release is unproven, so keep normal pruning.
pub(super) struct RetainedChanges {
    repo: git2::Repository,
    head: git2::Oid,
    released: git2::Oid,
    package_files: Option<HashSet<Utf8PathBuf>>,
    paths: Vec<Utf8PathBuf>,
    cache: HashMap<String, Contribution>,
    parents: HashMap<String, Vec<String>>,
    root: Option<String>,
    boundaries: HashSet<String>,
    reachable: HashSet<String>,
}

#[derive(Clone, Copy)]
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
            cache: HashMap::new(),
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

    pub(super) fn contains(&mut self, commit: &str) -> bool {
        if !self.reaches(commit) {
            return false;
        }
        let contribution = if let Some(contribution) = self.cache.get(commit) {
            *contribution
        } else {
            let contribution = self.check(commit).unwrap_or_else(|error| {
                // Shallow histories may not contain the parent required for a revert.
                // In that case there is no evidence to override ancestry pruning.
                debug!("cannot check retained changes in {commit}: {error:#}");
                Contribution::Absent
            });
            self.cache.insert(commit.to_owned(), contribution);
            contribution
        };
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

    fn check(&self, commit: &str) -> anyhow::Result<Contribution> {
        let commit = self.repo.find_commit(git2::Oid::from_str(commit)?)?;
        let released = self.repo.find_commit(self.released)?;
        // A conflict against the release does not establish that the commit's
        // contribution was absent from it. Preserve the existing pruning then.
        if !matches!(
            self.undo_changes_package(&commit, &released)?,
            Contribution::Absent
        ) {
            return Ok(Contribution::Absent);
        }
        let head = self.repo.find_commit(self.head)?;
        // Once absence from the release is established, a conflict at HEAD is
        // ambiguous: keep the commit rather than losing a breaking-change marker.
        self.undo_changes_package(&commit, &head)
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
            for entry in [conflict.ancestor, conflict.our, conflict.their]
                .into_iter()
                .flatten()
            {
                if std::str::from_utf8(&entry.path)
                    .map(|path| self.includes(Path::new(path)))
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
            [delta.old_file().path(), delta.new_file().path()]
                .into_iter()
                .flatten()
                .any(|path| self.includes(path))
        });
        Ok(if changed {
            Contribution::Present
        } else {
            Contribution::Absent
        })
    }

    fn includes(&self, path: &Path) -> bool {
        let Some(path) = Utf8Path::from_path(path) else {
            return true;
        };
        // Match package equality: lockfile changes are handled separately for
        // executables, and Cargo's generated metadata is not package source.
        if matches!(
            path.file_name(),
            Some("Cargo.lock" | CARGO_TOML_ORIG | CARGO_VCS_INFO)
        ) {
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
