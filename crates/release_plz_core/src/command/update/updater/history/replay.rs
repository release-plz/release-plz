use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
};

use anyhow::Context as _;
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use git_cmd::Repo;

use crate::fs_utils;

/// Replay changes onto other snapshots in an isolated libgit2 repository that
/// reads the source objects without writing to the source repository.
pub(super) struct ChangeReplay<'a> {
    /// The source repository, whose Git fetches the objects a partial clone
    /// lacks, see [`Self::fetch_missing_blobs`].
    repository: &'a Repo,
    repo: git2::Repository,
    /// The repository-relative paths the replayed trees are restricted to,
    /// see [`Self::restrict`].
    paths: Vec<Utf8PathBuf>,
}

impl<'a> ChangeReplay<'a> {
    /// Read the objects of `repository`, whose `head` commit id shows its object
    /// format: only SHA-1 repositories are supported, since libgit2 cannot read
    /// SHA-256 objects. Replays read only the repository-relative `paths`,
    /// see [`Self::restrict`].
    pub(super) fn new(
        repository: &'a Repo,
        head: &str,
        paths: &[&Utf8Path],
    ) -> anyhow::Result<Self> {
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
        Ok(Self {
            repository,
            repo,
            paths: paths.iter().map(|path| path.to_path_buf()).collect(),
        })
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
        let tree = self.tree(&commit)?;
        let parent = self.first_parent_tree(&commit)?;
        let target = self.tree(&self.commit(target)?)?;
        self.fetch_missing_blobs(&tree, &[&target, &parent])?;
        self.merging_affects_files(&tree, &target, &parent, conflicts, includes)
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
        let tree = self.tree(&commit)?;
        let target = self.tree(&self.commit(target)?)?;
        let edited = self.tree(&self.commit(edited)?)?;
        let parent = self.first_parent_tree(&commit)?;
        self.fetch_missing_blobs(&tree, &[&parent, &edited, &target])?;
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

    /// Fetch the blobs of `trees` that differ from `base`'s and are missing from
    /// the object database. A partial clone lacks the package blobs of commits
    /// the walk never checked out, such as a candidate's parent on an excluded
    /// branch; libgit2 cannot fetch them, but Git does on lookup. Full clones
    /// miss nothing and spawn no process.
    ///
    /// For every path, these diffs hold each distinct blob among the snapshots,
    /// so the merges and the rename detection find every blob they read. The
    /// trees are restricted to the package, so only its blobs are fetched.
    fn fetch_missing_blobs(
        &self,
        base: &git2::Tree<'_>,
        trees: &[&git2::Tree<'_>],
    ) -> anyhow::Result<()> {
        let odb = self.repo.odb()?;
        for tree in trees {
            // Without rename detection, the diff reads no blobs itself.
            let diff = self.repo.diff_tree_to_tree(Some(base), Some(tree), None)?;
            for delta in diff.deltas() {
                for (file, side) in [(delta.old_file(), base), (delta.new_file(), *tree)] {
                    // Deleted sides have a zero id. A miss refreshes the object
                    // database, so a blob fetched for one tree is found for the
                    // next.
                    if file.id().is_zero() || odb.exists(file.id()) {
                        continue;
                    }
                    // Submodules are commits of another repository. Read the
                    // mode from the tree entry: `DiffFile::mode` panics on
                    // legacy modes Git accepts, such as 100600.
                    if is_submodule(side, &file) {
                        continue;
                    }
                    self.repository
                        .git(&["cat-file", "-e", &file.id().to_string()])?;
                }
            }
        }
        Ok(())
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
        let mut rename_diff = self
            .repo
            .diff_tree_to_tree(Some(tree), Some(target), None)?;
        rename_diff.find_similar(Some(git2::DiffFindOptions::new().renames(true)))?;
        let renamed: HashMap<&[u8], &[u8]> = rename_diff
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

    /// The tree of `commit`, restricted as [`Self::restrict`] describes.
    fn tree<'r>(&'r self, commit: &git2::Commit<'r>) -> anyhow::Result<git2::Tree<'r>> {
        self.restrict(commit.tree()?)
    }

    /// The tree a change is relative to: the first parent's, as `git revert -m 1`
    /// undoes a merge, or an empty tree for a root commit.
    fn first_parent_tree<'r>(
        &'r self,
        commit: &git2::Commit<'r>,
    ) -> anyhow::Result<git2::Tree<'r>> {
        if commit.parent_count() == 0 {
            return self.empty_tree();
        }
        // Look the parent up explicitly: `Commit::parents` silently ends at a
        // parent that a shallow clone does not have.
        self.tree(&self.repo.find_commit(commit.parent_id(0)?)?)
    }

    fn empty_tree(&self) -> anyhow::Result<git2::Tree<'_>> {
        Ok(self.repo.find_tree(self.repo.treebuilder(None)?.write()?)?)
    }

    /// The entries of `tree` at the replayed paths, written to the in-memory
    /// backend, or `tree` itself when a path is the repository root.
    ///
    /// Merges and rename detection read the blobs of every path whose entries
    /// differ between the snapshots. A partial clone fetches blobs only when a
    /// checkout needs them, so files outside the package lack the blobs of the
    /// commits the walk skipped. Restricting the trees keeps the reads, and the
    /// fetches of [`Self::fetch_missing_blobs`], within the package.
    ///
    /// The restriction hides renames across the package boundary: a file the
    /// target moved out of the package shows as a modify/delete conflict, which
    /// conservatively counts as a change and retains the marker, and a file
    /// moved into the package is not matched to its old path.
    fn restrict<'r>(&'r self, tree: git2::Tree<'r>) -> anyhow::Result<git2::Tree<'r>> {
        if self.paths.iter().any(|path| path.as_str().is_empty()) {
            return Ok(tree);
        }
        let mut update = git2::build::TreeUpdateBuilder::new();
        for path in &self.paths {
            // A path the snapshot lacks selects nothing. The entries keep their
            // ids, so a merge reads the same blobs.
            if let Ok(entry) = tree.get_path(path.as_std_path()) {
                update.upsert(path.as_str(), entry.id(), file_mode(&entry)?);
            }
        }
        let id = update.create_updated(&self.repo, &self.empty_tree()?)?;
        Ok(self.repo.find_tree(id)?)
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

/// Whether `file`, one side of a delta, is a submodule in `tree`, that side's
/// tree. The tree entry's mode is normalized, unlike the delta's.
fn is_submodule(tree: &git2::Tree<'_>, file: &git2::DiffFile<'_>) -> bool {
    file.path()
        .and_then(|path| tree.get_path(path).ok())
        .is_some_and(|entry| entry.filemode() == i32::from(git2::FileMode::Commit))
}

/// The mode of `entry` as [`git2::build::TreeUpdateBuilder`] takes it.
fn file_mode(entry: &git2::TreeEntry<'_>) -> anyhow::Result<git2::FileMode> {
    use git2::FileMode::{Blob, BlobExecutable, BlobGroupWritable, Commit, Link, Tree};
    [Tree, Blob, BlobExecutable, Link, Commit, BlobGroupWritable]
        .into_iter()
        .find(|mode| i32::from(*mode) == entry.filemode())
        .with_context(|| {
            format!(
                "unknown file mode {:o} of {:?}",
                entry.filemode(),
                entry.name()
            )
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The repository-relative paths of the blobs of `tree`, in tree order.
    fn blob_paths(tree: &git2::Tree<'_>) -> Vec<String> {
        let mut paths = Vec::new();
        tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
            if entry.kind() == Some(git2::ObjectType::Blob) {
                paths.push(format!("{root}{}", entry.name().unwrap()));
            }
            git2::TreeWalkResult::Ok
        })
        .unwrap();
        paths
    }

    #[test]
    fn replayed_trees_hold_only_the_package_and_its_readme() {
        let dir = fs_utils::Utf8TempDir::new().unwrap();
        // Repo::init commits README.md at the repository root.
        let repo = Repo::init(dir.path());
        for path in [
            "crates/pkg/Cargo.toml",
            "crates/pkg/src/lib.rs",
            "crates/other/src/lib.rs",
            "docs/x",
        ] {
            let file = repo.directory().join(path);
            fs_err::create_dir_all(file.parent().unwrap()).unwrap();
            fs_err::write(file, path).unwrap();
        }
        repo.add_all_and_commit("add files").unwrap();
        let head = repo.current_commit_hash().unwrap();
        let restricted_blobs = |paths: &[&str]| {
            let paths: Vec<_> = paths.iter().map(Utf8Path::new).collect();
            let replay = ChangeReplay::new(&repo, &head, &paths).unwrap();
            let commit = replay.commit(&head).unwrap();
            let tree = replay.tree(&commit).unwrap();
            let blobs = blob_paths(&tree);
            // The selected entries keep their ids, so a merge reads the same
            // blobs, and the source repository stays untouched.
            for path in &blobs {
                let path = std::path::Path::new(path);
                assert_eq!(
                    tree.get_path(path).unwrap().id(),
                    commit.tree().unwrap().get_path(path).unwrap().id()
                );
            }
            if !tree.is_empty() && !paths.contains(&Utf8Path::new("")) {
                // Restricted trees stay in memory: the source repository lacks them.
                assert!(
                    repo.git(&["cat-file", "-e", &tree.id().to_string()])
                        .is_err(),
                    "{paths:?}"
                );
            }
            blobs
        };

        // A README outside the package directory and a missing path.
        assert_eq!(
            restricted_blobs(&["crates/pkg", "README.md", "missing/README.md"]),
            [
                "README.md",
                "crates/pkg/Cargo.toml",
                "crates/pkg/src/lib.rs"
            ]
        );
        // A README inside the package directory selects it once.
        assert_eq!(
            restricted_blobs(&["crates/pkg", "crates/pkg/Cargo.toml"]),
            ["crates/pkg/Cargo.toml", "crates/pkg/src/lib.rs"]
        );
        // The repository root selects everything, and only missing paths nothing.
        assert_eq!(
            restricted_blobs(&[""]),
            [
                "README.md",
                "crates/other/src/lib.rs",
                "crates/pkg/Cargo.toml",
                "crates/pkg/src/lib.rs",
                "docs/x",
            ]
        );
        assert!(restricted_blobs(&["missing"]).is_empty());
    }

    /// A commit that only gives a package file a legacy mode Git accepts, such
    /// as the 100600 old importers wrote. Git reads it as 100644, so undoing
    /// the commit leaves its parent unchanged.
    #[test]
    fn legacy_file_modes_do_not_abort_the_replay() {
        let dir = fs_utils::Utf8TempDir::new().unwrap();
        // Repo::init commits README.md at the repository root.
        let repo = Repo::init(dir.path());
        fs_err::write(repo.directory().join("f"), "hello\n").unwrap();
        repo.add_all_and_commit("base").unwrap();
        let base = repo.current_commit_hash().unwrap();
        // Git refuses to write such a tree unless told to take it literally.
        let mut tree = Vec::new();
        for (mode, path) in [("100644", "README.md"), ("100600", "f")] {
            let blob = repo.git(&["rev-parse", &format!("HEAD:{path}")]).unwrap();
            tree.extend_from_slice(format!("{mode} {path}\0").as_bytes());
            tree.extend_from_slice(git2::Oid::from_str(&blob).unwrap().as_bytes());
        }
        let tree_file = dir.path().join("tree.bin");
        fs_err::write(&tree_file, &tree).unwrap();
        let tree = repo
            .git(&[
                "hash-object",
                "-t",
                "tree",
                "-w",
                "--literally",
                tree_file.as_str(),
            ])
            .unwrap();
        let commit = repo
            .git(&["commit-tree", &tree, "-p", "HEAD", "-m", "legacy mode"])
            .unwrap();
        repo.git(&["update-ref", "HEAD", &commit]).unwrap();
        let replay = ChangeReplay::new(&repo, &commit, &[Utf8Path::new("")]).unwrap();
        let affects = replay
            .undo_affects_package(&commit, &base, TokenConflicts::Unresolved, |_| true)
            .unwrap();
        assert!(!affects);
    }
}
