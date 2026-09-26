use std::{collections::HashMap, fmt::Write as _, path::Path, process::Command};

use anyhow::Context as _;
use git_cmd::Repo;

use crate::fs_utils;

/// An isolated Git directory reads the source objects and stores replay results
/// separately. The CLI supports both SHA-1 and SHA-256 without touching the
/// source's index, worktree, configuration, or object database.
pub(in super::super) struct Replay {
    directory: fs_utils::Utf8TempDir,
}

impl Replay {
    pub(super) fn new(repository: &Repo, object_format: &str) -> anyhow::Result<Self> {
        let replay = Self {
            directory: fs_utils::Utf8TempDir::new()?,
        };
        let help = replay
            .command()
            .args(["merge-tree", "-h"])
            .output()
            .context("cannot run git merge-tree")?;
        anyhow::ensure!(
            [help.stdout, help.stderr].iter().any(|output| output
                .windows(b"merge-base".len())
                .any(|part| part == b"merge-base")),
            "checking retained changes requires Git 2.40 or newer (git merge-tree --merge-base)"
        );
        replay.git(&[
            "init",
            "--bare",
            "--template=",
            &format!("--object-format={object_format}"),
        ])?;
        let objects = repository.git(&[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "objects",
        ])?;
        // Quote the path using Git's C-style syntax, including control characters.
        let mut quoted = String::from("\"");
        for character in objects.chars() {
            match character {
                '\\' | '"' => {
                    quoted.push('\\');
                    quoted.push(character);
                }
                '\0'..='\x1f' | '\x7f' => {
                    write!(quoted, "\\{:03o}", u32::from(character))?;
                }
                _ => quoted.push(character),
            }
        }
        quoted.push_str("\"\n");
        fs_err::write(
            replay.directory.path().join("objects/info/alternates"),
            quoted,
        )?;
        fs_err::create_dir_all(replay.directory.path().join("info"))?;
        // Highest-precedence attributes override .gitattributes in every tree.
        fs_err::write(
            replay.directory.path().join("info/attributes"),
            "* merge=text\n",
        )?;
        Ok(replay)
    }

    fn command(&self) -> Command {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(self.directory.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                self.directory.path().join("empty-config"),
            )
            .env("GIT_CONFIG_COUNT", "0")
            .env("GIT_ATTR_NOSYSTEM", "1")
            .env_remove("GIT_CONFIG_PARAMETERS")
            .env_remove("GIT_CONFIG")
            .env_remove("GIT_DIR")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .env_remove("GIT_ATTR_SOURCE")
            .stdin(std::process::Stdio::null());
        command
    }

    fn git(&self, args: &[&str]) -> anyhow::Result<Vec<u8>> {
        let output = self
            .command()
            .args(args)
            .output()
            .with_context(|| format!("cannot run git {args:?}"))?;
        anyhow::ensure!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output.stdout)
    }

    pub(super) fn change<'a>(&'a self, commit: &'a str) -> anyhow::Result<Change<'a>> {
        Ok(Change {
            replay: self,
            commit,
            parent: self.first_parent(commit)?,
        })
    }

    fn first_parent(&self, commit: &str) -> anyhow::Result<String> {
        // Read the actual header: rev-list treats shallow boundaries as roots.
        let contents = self.git(&["cat-file", "commit", commit])?;
        if let Some(parent) = contents
            .split(|byte| *byte == b'\n')
            .take_while(|line| !line.is_empty())
            .find_map(|line| line.strip_prefix(b"parent "))
        {
            return Ok(std::str::from_utf8(parent)?.to_owned());
        }
        // Git 2.40 requires commit tips, so give a root's empty parent a commit.
        let tree = self.git(&["mktree"])?;
        let tree = std::str::from_utf8(&tree)?.trim();
        let output = self
            .command()
            .args(["commit-tree", tree, "-m", "empty replay parent"])
            .env("GIT_AUTHOR_NAME", "release-plz")
            .env("GIT_AUTHOR_EMAIL", "release-plz@example.invalid")
            .env("GIT_COMMITTER_NAME", "release-plz")
            .env("GIT_COMMITTER_EMAIL", "release-plz@example.invalid")
            .output()
            .context("cannot create empty replay parent")?;
        anyhow::ensure!(
            output.status.success(),
            "cannot create empty replay parent: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(std::str::from_utf8(&output.stdout)?.trim().to_owned())
    }
}

pub(in super::super) struct Change<'a> {
    replay: &'a Replay,
    commit: &'a str,
    parent: String,
}

impl Change<'_> {
    /// Whether undoing `commit` on `target` changes the packaged files. Unresolved
    /// conflicts count as changes. Supported text conflicts are retried at
    /// token granularity, favoring the target only for the release check.
    pub(super) fn undo_changes_package(
        &self,
        target: &str,
        favor_target: bool,
        includes: impl Fn(&Path) -> bool,
    ) -> anyhow::Result<bool> {
        let Self {
            replay,
            commit,
            parent,
        } = self;
        // Reversing the change is a three-way merge with the changed commit as
        // base and its first parent as the other side (`git revert -m 1`).
        let output = replay
            .command()
            .args([
                "merge-tree",
                "--write-tree",
                "-z",
                "--messages",
                &format!("--merge-base={commit}"),
                target,
                parent,
            ])
            .output()
            .context("cannot run git merge-tree")?;
        anyhow::ensure!(
            matches!(output.status.code(), Some(0 | 1)),
            "git merge-tree failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut records = output.stdout.split(|byte| *byte == 0);
        let tree = std::str::from_utf8(records.next().context("missing merge tree")?)?;
        let mut conflicts: HashMap<&[u8], [Option<ConflictEntry>; 3]> = HashMap::new();
        for record in records.by_ref().take_while(|record| !record.is_empty()) {
            let separator = record
                .iter()
                .position(|byte| *byte == b'\t')
                .context("missing conflict path")?;
            let (metadata, path) = (&record[..separator], &record[separator + 1..]);
            let mut metadata = std::str::from_utf8(metadata)?.split(' ');
            let mode = metadata.next().context("missing conflict mode")?;
            let object = metadata.next().context("missing conflict object")?;
            let stage: usize = metadata.next().context("missing conflict stage")?.parse()?;
            anyhow::ensure!((1..=3).contains(&stage), "invalid conflict stage {stage}");
            conflicts.entry(path).or_default()[stage - 1] = Some(ConflictEntry {
                mode: mode.to_owned(),
                object: object.to_owned(),
            });
        }
        let includes = |path: &[u8]| {
            std::str::from_utf8(path)
                .map(|path| includes(Path::new(path)))
                .unwrap_or(true)
        };
        // Structural conflicts can have no unmerged stages at all (for example
        // a directory rename). Their stable, machine-readable messages also
        // retain the original paths when a rename moved the staged entries.
        while let Some(count) = records.next().filter(|record| !record.is_empty()) {
            let count: usize = std::str::from_utf8(count)?.parse()?;
            let mut affects_package = count == 0;
            for _ in 0..count {
                affects_package |=
                    includes(records.next().context("missing conflict message path")?);
            }
            let kind = records.next().context("missing conflict message type")?;
            records.next().context("missing conflict message detail")?;
            if affects_package && kind.starts_with(b"CONFLICT") && kind != b"CONFLICT (contents)" {
                return Ok(true);
            }
        }
        for (path, conflict) in &conflicts {
            if includes(path) && !self.conflict_leaves_file_unchanged(conflict, favor_target)? {
                return Ok(true);
            }
        }
        let changed = self.replay.git(&[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            "-z",
            "--no-renames",
            target,
            tree,
        ])?;
        Ok(changed.split(|byte| *byte == 0)
            // Conflicted paths were checked above, including nonconflicting hunks.
            .filter(|path| !path.is_empty() && !conflicts.contains_key(path))
            .any(includes))
    }

    /// Binary, large, rename, deletion, and mode conflicts cannot establish absence.
    fn conflict_leaves_file_unchanged(
        &self,
        conflict: &[Option<ConflictEntry>; 3],
        favor_target: bool,
    ) -> anyhow::Result<bool> {
        let [Some(ancestor), Some(ours), Some(theirs)] = conflict else {
            return Ok(false);
        };
        if ancestor.mode != ours.mode
            || theirs.mode != ours.mode
            || !matches!(ours.mode.as_str(), "100644" | "100755")
        {
            return Ok(false);
        }
        let blobs = [
            self.replay.git(&["cat-file", "blob", &ancestor.object])?,
            self.replay.git(&["cat-file", "blob", &ours.object])?,
            self.replay.git(&["cat-file", "blob", &theirs.object])?,
        ];
        super::conflict_leaves_file_unchanged(blobs.each_ref().map(Vec::as_slice), favor_target)
    }
}

struct ConflictEntry {
    mode: String,
    object: String,
}
