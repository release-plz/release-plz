use std::{fmt::Write as _, path::Path};

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
        let object_format = repository.git(&["rev-parse", "--show-object-format"])?;
        if object_format == "sha1" {
            Ok(Self::Libgit2(libgit2::Replay::new(repository)?))
        } else {
            Ok(Self::Cli(cli::Replay::new(repository, &object_format)?))
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
