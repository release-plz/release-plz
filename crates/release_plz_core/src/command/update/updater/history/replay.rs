use std::{fmt::Write as _, path::Path};

use anyhow::Context as _;
use cargo_metadata::camino::Utf8PathBuf;
use git_cmd::Repo;

mod cli;
mod libgit2;

/// Use native replay where libgit2 supports the repository's object format.
/// Both backends read source objects without writing to the source repository.
pub(super) enum ChangeReplay {
    Libgit2(libgit2::Replay),
    Cli(cli::Replay),
}

impl ChangeReplay {
    pub(super) fn new(repository: &Repo) -> anyhow::Result<Self> {
        let object_format = object_format(repository)?;
        let objects = objects_directory(repository)?;
        if object_format == "sha1" {
            Ok(Self::Libgit2(libgit2::Replay::new(&objects)?))
        } else {
            Ok(Self::Cli(cli::Replay::new(&objects, object_format)?))
        }
    }

    /// Prepare the change once for replay against both the release and HEAD.
    pub(super) fn change<'a>(&'a self, commit: &'a str) -> anyhow::Result<Change<'a>> {
        match self {
            Self::Libgit2(replay) => Ok(Change::Libgit2(replay.change(commit)?)),
            Self::Cli(replay) => Ok(Change::Cli(replay.change(commit)?)),
        }
    }
}

pub(super) enum Change<'a> {
    Libgit2(libgit2::Change<'a>),
    Cli(cli::Change<'a>),
}

impl Change<'_> {
    /// Whether undoing this change affects packaged files. Unresolved conflicts
    /// count as changes. Text conflicts are retried at token granularity,
    /// favoring the target only when checking the release.
    pub(super) fn undo_changes_package(
        &self,
        target: &str,
        favor_target: bool,
        includes: impl Fn(&Path) -> bool,
    ) -> anyhow::Result<bool> {
        match self {
            Self::Libgit2(change) => change.undo_changes_package(target, favor_target, includes),
            Self::Cli(change) => change.undo_changes_package(target, favor_target, includes),
        }
    }
}

/// The object format of `repository`: `sha1` or `sha256`.
///
/// `git rev-parse` echoes an option it does not know instead of failing, and
/// `--show-object-format` needs Git 2.29, so accept only the formats Git has.
fn object_format(repository: &Repo) -> anyhow::Result<&'static str> {
    let format = repository.git(&["rev-parse", "--show-object-format"])?;
    ["sha1", "sha256"]
        .into_iter()
        .find(|known| *known == format)
        .with_context(|| format!("unknown object format {format:?}: Git 2.29 or newer is required"))
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
/// edits on the same line still apply; conflicting tokens keep the target's
/// content only when checking the release.
fn conflict_leaves_file_unchanged(blobs: [&[u8]; 3], favor_target: bool) -> anyhow::Result<bool> {
    let Some(encoded) = encode_conflict(blobs)? else {
        return Ok(false);
    };
    let [mut ancestor, mut ours, mut theirs] = std::array::from_fn(|_| git2::MergeFileInput::new());
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
