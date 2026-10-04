use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
};

use cargo_metadata::camino::Utf8PathBuf;
use git_cmd::Repo;

use crate::fs_utils;

/// Replay changes onto other snapshots in an isolated libgit2 repository that
/// reads the source objects without writing to the source repository.
pub(super) struct ChangeReplay {
    repo: git2::Repository,
}

impl ChangeReplay {
    /// Read the objects of `repository`, whose `head` commit id shows its object
    /// format: only SHA-1 repositories are supported, since libgit2 cannot read
    /// SHA-256 objects.
    pub(super) fn new(repository: &Repo, head: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(
            git2::Oid::from_str(head).is_ok(),
            "SHA-256 repositories are not supported"
        );
        let objects = objects_directory(repository)?;
        // Alternates are read-only. Store synthetic attributes and replay results
        // in memory so no objects are added to the source, including worktrees.
        // The results of every replay stay in memory until this replay is
        // dropped with the walk of its package.
        let odb = git2::Odb::new()?;
        odb.add_disk_alternate(objects.as_str())?;
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

    /// Whether undoing the change of `commit` at `target` affects the files
    /// `includes` selects, as [`Self::merging_affects_files`] counts changes.
    pub(super) fn undo_affects_package(
        &self,
        commit: &str,
        target: &str,
        conflicts: TokenConflicts,
        includes: impl Fn(&[u8]) -> bool,
    ) -> anyhow::Result<bool> {
        let commit = self.commit(commit)?;
        let parent = self.first_parent_tree(&commit)?;
        let target = self.commit(target)?.tree()?;
        self.merging_affects_files(&commit.tree()?, &target, &parent, conflicts, includes)
    }

    /// Whether applying the edits made from `commit` to `edited` onto `target`
    /// affects the files of the change of `commit` that `includes` selects, as
    /// [`Self::merging_affects_files`] counts changes. Files `target` renamed
    /// since `commit` count under their new name, where the merge applies the
    /// edits.
    pub(super) fn edits_affect_package(
        &self,
        commit: &str,
        edited: &str,
        target: &str,
        conflicts: TokenConflicts,
        includes: impl Fn(&[u8]) -> bool,
    ) -> anyhow::Result<bool> {
        let commit = self.commit(commit)?;
        let tree = commit.tree()?;
        let target = self.commit(target)?.tree()?;
        let edited = self.commit(edited)?.tree()?;
        let changed = self.changed_paths(&self.first_parent_tree(&commit)?, &tree, &target)?;
        self.merging_affects_files(&tree, &target, &edited, conflicts, |path| {
            changed.contains(path) && includes(path)
        })
    }

    /// The paths of the change from `parent` to `tree`, together with the paths
    /// `target` renamed them to: a merge applies edits to a renamed file there.
    fn changed_paths(
        &self,
        parent: &git2::Tree<'_>,
        tree: &git2::Tree<'_>,
        target: &git2::Tree<'_>,
    ) -> anyhow::Result<HashSet<Vec<u8>>> {
        let mut renamed = self
            .repo
            .diff_tree_to_tree(Some(tree), Some(target), None)?;
        renamed.find_similar(Some(git2::DiffFindOptions::new().renames(true)))?;
        let renamed: HashMap<&[u8], &[u8]> = renamed
            .deltas()
            .filter(|delta| delta.status() == git2::Delta::Renamed)
            .filter_map(|delta| {
                Some((
                    delta.old_file().path_bytes()?,
                    delta.new_file().path_bytes()?,
                ))
            })
            .collect();
        let changed = self
            .repo
            .diff_tree_to_tree(Some(parent), Some(tree), None)?;
        Ok(changed
            .deltas()
            .flat_map(|delta| delta_paths(&delta))
            .flat_map(|path| std::iter::once(path).chain(renamed.get(path).copied()))
            .map(<[u8]>::to_vec)
            .collect())
    }

    fn commit(&self, id: &str) -> anyhow::Result<git2::Commit<'_>> {
        Ok(self.repo.find_commit(git2::Oid::from_str(id)?)?)
    }

    /// The tree a change is relative to: the first parent's, as `git revert -m 1`
    /// undoes a merge, or an empty tree for a root commit.
    fn first_parent_tree<'r>(
        &'r self,
        commit: &git2::Commit<'r>,
    ) -> anyhow::Result<git2::Tree<'r>> {
        if commit.parent_count() == 0 {
            return Ok(self.repo.find_tree(self.repo.treebuilder(None)?.write()?)?);
        }
        // Look the parent up explicitly: `Commit::parents` silently ends at a
        // parent that a shallow clone does not have.
        Ok(self.repo.find_commit(commit.parent_id(0)?)?.tree()?)
    }

    /// Whether merging the edits made from `base` to `theirs` into `ours` changes
    /// the files `includes` selects. Unresolved conflicts count as changes. Text
    /// conflicts are retried at token granularity, resolved as `conflicts` says.
    fn merging_affects_files(
        &self,
        base: &git2::Tree<'_>,
        ours: &git2::Tree<'_>,
        theirs: &git2::Tree<'_>,
        conflicts: TokenConflicts,
        includes: impl Fn(&[u8]) -> bool,
    ) -> anyhow::Result<bool> {
        let index = self.repo.merge_trees(base, ours, theirs, None)?;
        for conflict in index.conflicts()? {
            let conflict = conflict?;
            let affects_package = [
                conflict.ancestor.as_ref(),
                conflict.our.as_ref(),
                conflict.their.as_ref(),
            ]
            .into_iter()
            .flatten()
            .any(|entry| includes(&entry.path));
            if affects_package && !self.conflict_leaves_file_unchanged(&conflict, conflicts)? {
                return Ok(true);
            }
        }
        let diff = self
            .repo
            .diff_tree_to_index(Some(ours), Some(&index), None)?;
        Ok(diff
            .deltas()
            // Conflicted paths were checked above, including nonconflicting hunks.
            .filter(|delta| delta.status() != git2::Delta::Conflicted)
            .any(|delta| delta_paths(&delta).any(&includes)))
    }

    /// Binary, large, rename, deletion and mode conflicts cannot establish absence.
    fn conflict_leaves_file_unchanged(
        &self,
        conflict: &git2::IndexConflict,
        conflicts: TokenConflicts,
    ) -> anyhow::Result<bool> {
        let (Some(ancestor), Some(ours), Some(theirs)) =
            (&conflict.ancestor, &conflict.our, &conflict.their)
        else {
            return Ok(false);
        };
        if ancestor.path != ours.path
            || theirs.path != ours.path
            || !same_regular_file_mode([ancestor.mode, ours.mode, theirs.mode])
        {
            return Ok(false);
        }
        let blobs = [
            self.repo.find_blob(ancestor.id)?,
            self.repo.find_blob(ours.id)?,
            self.repo.find_blob(theirs.id)?,
        ];
        text_conflict_leaves_file_unchanged(blobs.each_ref().map(git2::Blob::content), conflicts)
    }
}

/// How a merge treats tokens that conflict with the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TokenConflicts {
    /// Keep the target's tokens: a change can be absent even when its inverse
    /// conflicts with edits next to it.
    FavorTarget,
    /// Leave them unresolved, so that they count as changes.
    Keep,
}

/// The old and the new path of `delta`, as Git reports them.
fn delta_paths<'a>(delta: &git2::DiffDelta<'a>) -> impl Iterator<Item = &'a [u8]> + use<'a> {
    [delta.old_file().path_bytes(), delta.new_file().path_bytes()]
        .into_iter()
        .flatten()
}

/// The absolute path of the object database of `repository`.
///
/// Let Git resolve it: libgit2 cannot open repositories with some valid
/// extensions, such as `extensions.partialClone`. Git reports the path relative
/// to the repository directory unless it is absolute, as in linked worktrees.
fn objects_directory(repository: &Repo) -> anyhow::Result<Utf8PathBuf> {
    let objects = repository.git(&["rev-parse", "--git-path", "objects"])?;
    fs_utils::canonicalize_utf8(&repository.directory().join(objects))
}

/// Whether every stage of a conflict is a regular file of the same mode, so
/// that merging their contents as text is meaningful. Type, mode, rename and
/// deletion conflicts cannot establish absence.
fn same_regular_file_mode(modes: [u32; 3]) -> bool {
    modes.iter().all(|mode| *mode == modes[0]) && matches!(modes[0], 0o100_644 | 0o100_755)
}

/// Whether undoing a text conflict leaves the target unchanged. Independent
/// edits on the same line still apply; `conflicts` decides about tokens that
/// do conflict.
fn text_conflict_leaves_file_unchanged(
    blobs: [&[u8]; 3],
    conflicts: TokenConflicts,
) -> anyhow::Result<bool> {
    let Some(encoded) = encode_conflict(blobs)? else {
        return Ok(false);
    };
    let [mut ancestor, mut ours, mut theirs] = std::array::from_fn(|_| git2::MergeFileInput::new());
    ancestor.content(encoded[0].as_bytes());
    ours.content(encoded[1].as_bytes());
    theirs.content(encoded[2].as_bytes());
    let mut options = git2::MergeFileOptions::new();
    if conflicts == TokenConflicts::FavorTarget {
        options.favor(git2::FileFavor::Ours);
    }
    let merged = git2::merge_file(&ancestor, &ours, &theirs, Some(&mut options))?;
    Ok(merged.is_automergeable() && merged.content() == encoded[1].as_bytes())
}

/// Put words with their trailing punctuation and other characters on separate lines.
/// Keeping identifiers whole avoids aligning their letters with unrelated edits;
/// trailing punctuation distinguishes a parameter's `bool)` from a local's `bool`.
/// Bound the expanded input and leave binary or non-UTF-8 content unresolved.
fn encode_conflict(blobs: [&[u8]; 3]) -> anyhow::Result<Option<[String; 3]>> {
    let max_conflict_input_bytes = 1024 * 1024; // 1 MiB across all three snapshots.
    if blobs.iter().map(|blob| blob.len()).sum::<usize>() > max_conflict_input_bytes
        || blobs.iter().any(|blob| blob.contains(&0))
    {
        return Ok(None);
    }
    let mut encoded = std::array::from_fn(|_| String::new());
    for (blob, tokens) in blobs.into_iter().zip(&mut encoded) {
        let Ok(contents) = std::str::from_utf8(blob) else {
            return Ok(None);
        };
        tokens.reserve(contents.len() * 4);
        let mut in_word = false;
        let mut word_token = false;
        for character in contents.chars() {
            let is_word = character.is_alphanumeric() || character == '_';
            let continues_token =
                (in_word && is_word) || (word_token && !is_word && !character.is_whitespace());
            if !continues_token {
                if !tokens.is_empty() {
                    tokens.push('\n');
                }
                word_token = is_word;
            }
            write!(tokens, "{:x},", u32::from(character))?;
            in_word = is_word;
        }
        tokens.push('\n');
    }
    Ok(Some(encoded))
}
