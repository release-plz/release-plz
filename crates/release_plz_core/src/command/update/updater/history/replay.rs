use std::fmt::Write as _;

use cargo_metadata::camino::Utf8PathBuf;
use git_cmd::Repo;

/// Undo changes in an isolated libgit2 repository that reads the source
/// objects without writing to the source repository.
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
    /// `includes` selects. Unresolved conflicts count as changes. Text conflicts
    /// are retried at token granularity, resolved as `conflicts` says.
    pub(super) fn undo_changes_package(
        &self,
        commit: &str,
        target: &str,
        conflicts: TokenConflicts,
        includes: impl Fn(&str) -> bool,
    ) -> anyhow::Result<bool> {
        let commit = self.repo.find_commit(git2::Oid::from_str(commit)?)?;
        let target = self.repo.find_commit(git2::Oid::from_str(target)?)?;
        // For merges, undo the change relative to the first parent, as
        // `git revert -m 1` does. libgit2 handles roots with an empty base.
        let mainline = u32::from(commit.parent_count() > 1);
        let index = self.repo.revert_commit(&commit, &target, mainline, None)?;
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
            .any(|entry| includes_bytes(&includes, &entry.path));
            if affects_package && !self.conflict_leaves_file_unchanged(&conflict, conflicts)? {
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
                [delta.old_file().path_bytes(), delta.new_file().path_bytes()]
                    .into_iter()
                    .flatten()
                    .any(|path| includes_bytes(&includes, path))
            }))
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

/// How undoing a change treats tokens that conflict with the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TokenConflicts {
    /// Keep the target's tokens: a change can be absent even when its inverse
    /// conflicts with edits next to it.
    FavorTarget,
    /// Leave them unresolved, so that they count as changes.
    Keep,
}

/// Apply `includes` to a path Git reports as bytes. A path that is not UTF-8
/// cannot match the package's file list, so it counts as packaged.
fn includes_bytes(includes: impl Fn(&str) -> bool, path: &[u8]) -> bool {
    std::str::from_utf8(path).map_or(true, includes)
}

/// The absolute path of the object database of `repository`.
///
/// Let Git resolve it: libgit2 cannot open repositories with some valid
/// extensions, such as `extensions.partialClone`. `git rev-parse` echoes an
/// option it does not know instead of failing, and `--path-format` needs Git
/// 2.31, so make sure the answer is a single existing absolute directory.
fn objects_directory(repository: &Repo) -> anyhow::Result<Utf8PathBuf> {
    let objects = Utf8PathBuf::from(repository.git(&[
        "rev-parse",
        "--path-format=absolute",
        "--git-path",
        "objects",
    ])?);
    anyhow::ensure!(
        !objects.as_str().contains('\n') && objects.is_absolute() && objects.is_dir(),
        "cannot locate the object database, git rev-parse reported {objects:?}: Git 2.31 or newer is required"
    );
    Ok(objects)
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

/// Put words and individual punctuation/whitespace characters on separate lines.
/// Keeping identifiers whole avoids aligning their letters with unrelated edits.
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
        for character in contents.chars() {
            let is_word = character.is_alphanumeric() || character == '_';
            if !tokens.is_empty() && !(in_word && is_word) {
                tokens.push('\n');
            }
            write!(tokens, "{:x},", u32::from(character))?;
            in_word = is_word;
        }
        tokens.push('\n');
    }
    Ok(Some(encoded))
}
