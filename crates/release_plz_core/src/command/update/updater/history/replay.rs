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
    /// affects the change of `commit` in the files `includes` selects, as
    /// [`Self::merging_affects_files`] counts changes. Independent text edits
    /// within those files do not count. Files `target` renamed
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
        let parent = self.first_parent_tree(&commit)?;
        let mut changed = HashSet::new();
        for (path, target_path) in self.changed_paths(&parent, &tree, &target)? {
            if (includes(&path) || includes(&target_path))
                && !self.edited_tokens_leave_file_unchanged(
                    [&parent, &tree, &edited, &target],
                    &path,
                    &target_path,
                    conflicts,
                )?
            {
                changed.extend(
                    [path, target_path]
                        .into_iter()
                        .filter(|path| includes(path)),
                );
            }
        }
        self.merging_affects_files(&tree, &target, &edited, conflicts, |path| {
            changed.contains(path)
        })
    }

    /// Prove that the target adopted the edits to the candidate's changed
    /// tokens, independently of other edits in the same file. Missing files,
    /// mode changes and unsupported text still need the ordinary tree replay.
    fn edited_tokens_leave_file_unchanged(
        &self,
        trees: [&git2::Tree<'_>; 4],
        path: &[u8],
        target_path: &[u8],
        conflicts: TokenConflicts,
    ) -> anyhow::Result<bool> {
        let (Ok(path), Ok(target_path)) =
            (std::str::from_utf8(path), std::str::from_utf8(target_path))
        else {
            return Ok(false);
        };
        // A rename in the target changes only its lookup; the candidate and
        // release still describe the contents at the original path.
        let paths = [path, path, path, target_path];
        let entries =
            std::array::from_fn(|i| trees[i].get_path(std::path::Path::new(paths[i])).ok());
        let [Some(parent), Some(changed), Some(edited), Some(target)] = entries else {
            return Ok(false);
        };
        let entries = [parent, changed, edited, target];
        if !same_regular_file_mode(
            entries
                .each_ref()
                .map(|entry| entry.filemode().cast_unsigned()),
        ) {
            return Ok(false);
        }
        let blobs = entries
            .iter()
            .map(|entry| self.repo.find_blob(entry.id()))
            .collect::<Result<Vec<_>, _>>()?;
        let Some([parent, changed, edited, target]) =
            encode_conflict(std::array::from_fn(|i| blobs[i].content()))?
        else {
            return Ok(false);
        };
        // Reapply the candidate to the release, keeping independent release
        // edits. The difference back to the release now describes only the
        // release's version of the candidate, without unrelated changes.
        let reapplied = merge_text(
            [parent.as_bytes(), changed.as_bytes(), edited.as_bytes()],
            TokenConflicts::FavorTarget,
        )?;
        if !reapplied.is_automergeable() {
            return Ok(false);
        }
        let merged = merge_text(
            [reapplied.content(), target.as_bytes(), edited.as_bytes()],
            conflicts,
        )?;
        Ok(merged.is_automergeable() && merged.content() == target.as_bytes())
    }

    /// The paths of the change from `parent` to `tree`, mapped to their paths
    /// in `target`: a merge applies edits to a renamed file at its new path.
    fn changed_paths(
        &self,
        parent: &git2::Tree<'_>,
        tree: &git2::Tree<'_>,
        target: &git2::Tree<'_>,
    ) -> anyhow::Result<HashMap<Vec<u8>, Vec<u8>>> {
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
            .map(|path| {
                (
                    path.to_vec(),
                    renamed.get(path).copied().unwrap_or(path).to_vec(),
                )
            })
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
    Unresolved,
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
fn same_regular_file_mode<const N: usize>(modes: [u32; N]) -> bool {
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
    let [ancestor, ours, theirs] = encoded.each_ref().map(|text| text.as_bytes());
    let merged = merge_text([ancestor, ours, theirs], conflicts)?;
    if merged.is_automergeable() {
        return Ok(merged.content() == ours);
    }
    if conflicts != TokenConflicts::Unresolved {
        return Ok(false);
    }
    // Repeated tokens can align an edit with an unrelated later insertion.
    // Replay the opposite edit first, then the original, requiring clean merges
    // and exact restoration of the target. For an undo this reapplies the whole
    // candidate before undoing it: any retained portion is removed as well, so
    // a partly retained change still affects the target.
    let applied = merge_text([theirs, ours, ancestor], TokenConflicts::Unresolved)?;
    if !applied.is_automergeable() {
        return Ok(false);
    }
    let undone = merge_text(
        [ancestor, applied.content(), theirs],
        TokenConflicts::Unresolved,
    )?;
    Ok(undone.is_automergeable() && undone.content() == ours)
}

fn merge_text(
    blobs: [&[u8]; 3],
    conflicts: TokenConflicts,
) -> anyhow::Result<git2::MergeFileResult> {
    let [mut ancestor, mut ours, mut theirs] = std::array::from_fn(|_| git2::MergeFileInput::new());
    ancestor.content(blobs[0]);
    ours.content(blobs[1]);
    theirs.content(blobs[2]);
    let mut options = git2::MergeFileOptions::new();
    if conflicts == TokenConflicts::FavorTarget {
        options.favor(git2::FileFavor::Ours);
    }
    Ok(git2::merge_file(
        &ancestor,
        &ours,
        &theirs,
        Some(&mut options),
    )?)
}

/// Put words with their trailing punctuation and other characters on separate lines.
/// Keeping identifiers whole avoids aligning their letters with unrelated edits;
/// trailing punctuation distinguishes a parameter's `bool)` from a local's `bool`.
/// Bound the expanded input and leave binary or non-UTF-8 content unresolved.
fn encode_conflict<const N: usize>(blobs: [&[u8]; N]) -> anyhow::Result<Option<[String; N]>> {
    let max_conflict_input_bytes = 1024 * 1024; // 1 MiB across the snapshots.
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
