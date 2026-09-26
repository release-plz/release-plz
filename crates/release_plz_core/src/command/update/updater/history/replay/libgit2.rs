use std::path::Path;

use anyhow::Context as _;
use git_cmd::Repo;

pub(in super::super) struct Replay {
    repo: git2::Repository,
}

impl Replay {
    pub(super) fn new(repository: &Repo) -> anyhow::Result<Self> {
        let source = git2::Repository::open(repository.directory())?;
        let objects = source.commondir().join("objects");
        let objects = objects.to_str().context("non-UTF-8 object directory")?;
        // Alternates are read-only. Store synthetic attributes and replay results
        // in memory so no objects are added to the source, including worktrees.
        let odb = git2::Odb::new()?;
        odb.add_disk_alternate(objects)?;
        odb.add_new_mempack_backend(1000)?;
        let repo = git2::Repository::from_odb(odb)?;
        // Remove user configuration and force the built-in text driver through
        // a synthetic index, overriding worktree, global and system attributes.
        repo.set_config(&git2::Config::new()?)?;
        let mut index = git2::Index::new()?;
        {
            let mut tree = repo.treebuilder(None)?;
            tree.insert(".gitattributes", repo.blob(b"* merge=text\n")?, 0o100_644)?;
            index.read_tree(&repo.find_tree(tree.write()?)?)?;
        }
        repo.set_index(&mut index)?;
        Ok(Self { repo })
    }

    pub(super) fn change(&self, commit: &str) -> anyhow::Result<Change<'_>> {
        Ok(Change {
            repo: &self.repo,
            commit: self.repo.find_commit(git2::Oid::from_str(commit)?)?,
        })
    }
}

pub(in super::super) struct Change<'a> {
    repo: &'a git2::Repository,
    commit: git2::Commit<'a>,
}

impl Change<'_> {
    pub(super) fn undo_changes_package(
        &self,
        target: &str,
        favor_target: bool,
        includes: impl Fn(&Path) -> bool,
    ) -> anyhow::Result<bool> {
        let target = self.repo.find_commit(git2::Oid::from_str(target)?)?;
        // For merges, undo the change relative to the first parent, as
        // `git revert -m 1` does. libgit2 handles roots with an empty base.
        let mainline = u32::from(self.commit.parent_count() > 1);
        let index = self
            .repo
            .revert_commit(&self.commit, &target, mainline, None)?;
        let tree = target.tree()?;
        for conflict in index.conflicts()? {
            let conflict = conflict?;
            let affects_package = [
                conflict.ancestor.as_ref(),
                conflict.our.as_ref(),
                conflict.their.as_ref(),
            ]
            .into_iter()
            .flatten()
            .any(|entry| {
                std::str::from_utf8(&entry.path)
                    .map(|path| includes(Path::new(path)))
                    .unwrap_or(true)
            });
            if affects_package && !self.conflict_leaves_file_unchanged(&conflict, favor_target)? {
                return Ok(true);
            }
        }
        let diff = self
            .repo
            .diff_tree_to_index(Some(&tree), Some(&index), None)?;
        Ok(diff
            .deltas()
            // Conflicted paths were checked above, including nonconflicting hunks.
            .filter(|delta| delta.status() != git2::Delta::Conflicted)
            .any(|delta| {
                [delta.old_file().path(), delta.new_file().path()]
                    .into_iter()
                    .flatten()
                    .any(&includes)
            }))
    }

    /// Binary, large, rename, deletion and mode conflicts cannot establish absence.
    fn conflict_leaves_file_unchanged(
        &self,
        conflict: &git2::IndexConflict,
        favor_target: bool,
    ) -> anyhow::Result<bool> {
        let (Some(ancestor), Some(ours), Some(theirs)) =
            (&conflict.ancestor, &conflict.our, &conflict.their)
        else {
            return Ok(false);
        };
        if ancestor.path != ours.path
            || theirs.path != ours.path
            || ancestor.mode != ours.mode
            || theirs.mode != ours.mode
            || !matches!(ours.mode, 0o100_644 | 0o100_755)
        {
            return Ok(false);
        }
        let blobs = [
            self.repo.find_blob(ancestor.id)?,
            self.repo.find_blob(ours.id)?,
            self.repo.find_blob(theirs.id)?,
        ];
        let Some(encoded) = super::encode_conflict(blobs.each_ref().map(git2::Blob::content))?
        else {
            return Ok(false);
        };
        let [mut ancestor, mut ours, mut theirs] =
            std::array::from_fn(|_| git2::MergeFileInput::new());
        ancestor.content(encoded[0].as_bytes());
        ours.content(encoded[1].as_bytes());
        theirs.content(encoded[2].as_bytes());
        let mut options = git2::MergeFileOptions::new();
        if favor_target {
            options.favor(git2::FileFavor::Ours);
        }
        let merged = git2::merge_file(&ancestor, &ours, &theirs, Some(&mut options))?;
        Ok(merged.is_automergeable() && merged.content() == encoded[1].as_bytes())
    }
}
