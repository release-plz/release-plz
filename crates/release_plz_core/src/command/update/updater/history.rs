use std::{fmt::Write as _, path::Path, process::Command};

use super::*;

/// Refine equality-based ancestry pruning with the changes still present at HEAD.
///
/// A merge can keep a change whose descendant on another branch matches the
/// release. Follow Git's simplified parent graph, stopping each lineage at equal
/// snapshots. For ancestors reachable through another lineage, revert the change
/// in an isolated object database to distinguish surviving contributions from
/// discarded merge parents. A revert that leaves the release unchanged proves a
/// change was absent there.
pub(super) struct RetainedChanges {
    replay: ChangeReplay,
    /// Repository-relative files Cargo packages at the release and, once
    /// [`Self::add_package_files`] ran, at HEAD. `None` when a listing failed:
    /// every file under `paths` counts then.
    package_files: Option<HashSet<Utf8PathBuf>>,
    /// Repository-relative paths: the package directory first, then the canonical
    /// target of the configured README, if any.
    paths: Vec<Utf8PathBuf>,
    /// Git's simplified, path-limited parent graph after release exclusions,
    /// with equal snapshot nodes removed to stop traversal at those boundaries.
    parents: HashMap<String, Vec<String>>,
    /// First commit of the simplified walk: HEAD only when HEAD touches the package.
    root: Option<String>,
    /// Commits reachable from `root` through the remaining parent graph.
    reachable: HashSet<String>,
    /// Full-history ancestors of every boundary: candidates for pruning.
    released_ancestors: HashSet<String>,
}

impl RetainedChanges {
    /// Start from the first equal snapshot `released` and its full-history
    /// `ancestors`, as in [`Self::add_boundary`].
    ///
    /// `package_files` are relative to the package directory, as Cargo lists
    /// them; `paths` are absolute, with the package directory first.
    pub(super) fn new(
        repository: &Repo,
        head: &str,
        released: &str,
        ancestors: Vec<String>,
        release_boundaries: &[&str],
        package_files: Option<HashSet<Utf8PathBuf>>,
        paths: &[Utf8PathBuf],
    ) -> anyhow::Result<Self> {
        // Follow the outer walk's simplification and release exclusions.
        let graph = repository.parents_at_paths(head, release_boundaries, paths)?;
        // The walked repository can be a temporary copy at a non-canonical path,
        // such as `/var` on macOS, while the README paths were canonicalized.
        // Canonicalization is best effort: the raw path is tried first anyway.
        let directory = repository.directory();
        let canonical_directory = crate::fs_utils::canonicalize_utf8(directory).ok();
        let relativize = |path: &Utf8Path| {
            [Some(directory), canonical_directory.as_deref()]
                .into_iter()
                .flatten()
                .find_map(|base| path.strip_prefix(base).ok())
                .map(Utf8Path::to_path_buf)
                .with_context(|| format!("{path} is outside the repository {directory}"))
        };
        let mut changes = Self {
            replay: ChangeReplay::new(repository, head, released)?,
            package_files: None,
            paths: paths
                .iter()
                .map(|path| relativize(path))
                .collect::<anyhow::Result<_>>()?,
            root: graph.first().map(|(commit, _)| commit.clone()),
            parents: graph.into_iter().collect(),
            reachable: HashSet::new(),
            released_ancestors: HashSet::new(),
        };
        changes.package_files = changes.repository_relative(package_files);
        changes.add_boundary(released, ancestors);
        Ok(changes)
    }

    /// Join Cargo's package-relative `files` onto the repository-relative
    /// package directory, so they compare with the paths Git reports.
    fn repository_relative(
        &self,
        files: Option<HashSet<Utf8PathBuf>>,
    ) -> Option<HashSet<Utf8PathBuf>> {
        let package = &self.paths[0];
        files.map(|files| files.into_iter().map(|file| package.join(file)).collect())
    }

    /// Register an equal snapshot together with its full-history `ancestors`.
    ///
    /// Full ancestry also includes branches discarded by merges. An ancestor can
    /// nevertheless survive through another lineage; lineage reachability and the
    /// content check of [`Self::retains`] preserve them.
    pub(super) fn add_boundary(&mut self, commit: &str, ancestors: Vec<String>) {
        // Remove only the boundary: its ancestors may still be reachable through
        // another lineage, which must remain available for the content check.
        self.parents.remove(commit);
        self.released_ancestors.extend(ancestors);
        self.reachable.clear();
        let mut pending: Vec<_> = self.root.iter().map(String::as_str).collect();
        while let Some(commit) = pending.pop() {
            let Some(parents) = self.parents.get(commit) else {
                continue;
            };
            if !self.reachable.insert(commit.to_owned()) {
                continue;
            }
            pending.extend(parents.iter().map(String::as_str));
        }
    }

    /// Add the package-relative files Cargo packages at another snapshot,
    /// typically HEAD: a file added or removed since the release is only listed
    /// on one side. A failed listing (`None`) makes every file under `paths` count.
    pub(super) fn add_package_files(&mut self, files: Option<HashSet<Utf8PathBuf>>) {
        self.package_files = self
            .package_files
            .take()
            .zip(self.repository_relative(files))
            .map(|(mut all, files)| {
                all.extend(files);
                all
            });
    }

    /// Whether the walk can skip `commit` without inspecting it: it is an
    /// ancestor of an equal snapshot and no other lineage reaches it without
    /// passing one. This is only an optimization; [`Self::retains`] makes the
    /// final decision with every discovered boundary.
    pub(super) fn skips(&self, commit: &str) -> bool {
        self.released_ancestors.contains(commit) && !self.reachable.contains(commit)
    }

    /// Whether `commit` stays in the diff: either it is not an ancestor of an
    /// equal snapshot, or another lineage reaches it without passing one and its
    /// change survives at HEAD. A simplified walk can visit an ancestor before
    /// the equal snapshot that prunes it, so this decision is load-bearing.
    pub(super) fn retains(&self, commit: &str) -> bool {
        if !self.released_ancestors.contains(commit) {
            return true;
        }
        // A sibling editing the same lines as a reverted commit can make its
        // undo conflict. Reachability ensures another lineage reaches the commit
        // without passing an equal package snapshot before trusting that.
        self.reachable.contains(commit)
            && self.survives(commit).unwrap_or_else(|error| {
                // Shallow histories may not contain the parent required for a
                // revert. Then there is no evidence to override ancestry pruning.
                warn!("cannot check retained changes in {commit}: {error:#}");
                false
            })
    }

    /// Whether the change of `commit` was absent from the release and is still
    /// present at HEAD.
    fn survives(&self, commit: &str) -> anyhow::Result<bool> {
        let parent = self.replay.first_parent(commit)?;
        // Resolve supported text conflicts in favor of the release: an
        // overwritten change can be absent even when its inverse conflicts.
        // Known limitation: when the release edited tokens adjacent to the
        // change, the token-level merge conflicts, favoring the release leaves
        // its content unchanged, and the change counts as absent. A commit also
        // reachable through another lineage can then be re-reported. Accepted:
        // it only reproduces the pre-existing behavior for that commit.
        if self.undo_changes_package(commit, &parent, &self.replay.released, true)? {
            return Ok(false);
        }
        // At HEAD, a clean token-level merge can establish that the change
        // was already undone despite later edits on the same line. Keep actual
        // token conflicts: an evolved change may still require its marker.
        self.undo_changes_package(commit, &parent, &self.replay.head, false)
    }

    /// Whether undoing `commit` on `target` changes the packaged files. Unresolved
    /// conflicts count as changes. Supported text conflicts are retried at
    /// token granularity, favoring the target only for the release check.
    fn undo_changes_package(
        &self,
        commit: &str,
        parent: &str,
        target: &str,
        favor_target: bool,
    ) -> anyhow::Result<bool> {
        // Reversing the change is a three-way merge with the changed commit as
        // base and its first parent as the other side (`git revert -m 1`).
        let output = self
            .replay
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
                .map(|path| self.includes(Path::new(path)))
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

    /// Whether a text conflict's inverse leaves the target unchanged. Independent
    /// edits on the same line still apply; conflicting tokens keep the
    /// target's content only when checking the release.
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
        let max_conflict_input_bytes = 1024 * 1024; // 1 MiB across all three snapshots.
        // Put words and individual punctuation/whitespace characters on separate
        // lines. Keeping identifiers whole avoids aligning their letters with
        // unrelated later edits. Bound the expanded input and leave binary or
        // non-UTF-8 content unresolved.
        if blobs.iter().map(Vec::len).sum::<usize>() > max_conflict_input_bytes
            || blobs.iter().any(|blob| blob.contains(&0))
        {
            return Ok(false);
        }
        let mut encoded = Vec::with_capacity(blobs.len());
        for blob in &blobs {
            let Ok(contents) = std::str::from_utf8(blob) else {
                return Ok(false);
            };
            let mut tokens = String::with_capacity(contents.len() * 4);
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
            encoded.push(tokens);
        }
        let files = fs_utils::Utf8TempDir::new()?;
        let paths = ["ancestor", "ours", "theirs"].map(|name| files.path().join(name));
        for (path, contents) in paths.iter().zip(&encoded) {
            fs_err::write(path, contents)?;
        }
        let mut command = self.replay.command();
        command.args(["merge-file", "-p"]);
        if favor_target {
            command.arg("--ours");
        }
        let output = command
            .arg("--")
            .args([&paths[1], &paths[0], &paths[2]])
            .output()
            .context("cannot run git merge-file")?;
        // merge-file returns the number of conflicts, capped at 127; errors
        // use negative exit codes, represented as 128 or greater by the shell.
        anyhow::ensure!(
            matches!(output.status.code(), Some(0..=127)),
            "git merge-file failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output.status.success() && output.stdout == encoded[1].as_bytes())
    }

    fn includes(&self, path: &Path) -> bool {
        let Some(path) = Utf8Path::from_path(path) else {
            return true;
        };
        if path
            .file_name()
            .is_some_and(crate::package_compare::is_generated_package_file)
        {
            return false;
        }
        if let Some(files) = &self.package_files {
            files.contains(path) || self.paths.iter().skip(1).any(|readme| path == readme)
        } else {
            self.paths.iter().any(|root| path.starts_with(root))
        }
    }
}

struct ConflictEntry {
    mode: String,
    object: String,
}

/// An isolated Git directory reads the source objects and stores replay results
/// separately. The CLI supports both SHA-1 and SHA-256 without touching the
/// source's index, worktree, configuration, or object database.
struct ChangeReplay {
    directory: fs_utils::Utf8TempDir,
    head: String,
    /// The first equal snapshot found by the walk, standing in for the release.
    released: String,
}

impl ChangeReplay {
    fn new(repository: &Repo, head: &str, released: &str) -> anyhow::Result<Self> {
        let replay = Self {
            directory: fs_utils::Utf8TempDir::new()?,
            head: head.to_owned(),
            released: released.to_owned(),
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
        let object_format = repository.git(&["rev-parse", "--show-object-format"])?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(path: &Utf8Path, object_format: &str) -> Repo {
        git_cmd::git_in_dir(path, &["init", &format!("--object-format={object_format}")]).unwrap();
        Repo::init(path)
    }

    /// Commit `contents` to `file` at the repository root and return the commit hash.
    fn commit_file(repo: &Repo, contents: &str) -> String {
        fs_err::write(repo.directory().join("file"), contents).unwrap();
        repo.add_all_and_commit("change file").unwrap();
        repo.current_commit_hash().unwrap()
    }

    /// Replay `changed` against the single equal snapshot `released`, treating
    /// the repository root as the package directory.
    fn replay(
        repo: &Repo,
        changed: &str,
        released: &str,
        package_files: Option<HashSet<Utf8PathBuf>>,
    ) -> RetainedChanges {
        RetainedChanges::new(
            repo,
            changed,
            released,
            vec![released.to_owned()],
            &[],
            package_files,
            &[repo.directory().to_path_buf()],
        )
        .unwrap()
    }

    #[test]
    fn global_merge_attributes_are_isolated() {
        let config = fs_utils::Utf8TempDir::new().unwrap();
        fs_err::create_dir(config.path().join("git")).unwrap();
        fs_err::write(config.path().join("git/attributes"), "* merge=union\n").unwrap();
        // The test harness matches on the path without the crate name.
        let child = format!(
            "{}::merge_configuration_is_isolated_without_changing_worktree_indexes",
            module_path!().split_once("::").unwrap().1
        );
        // Run the existing fixture in a fresh process: changing this process's
        // environment is not safe while other tests are running.
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &child])
            .env("XDG_CONFIG_HOME", config.path())
            .env("RELEASE_PLZ_TEST_GLOBAL_MERGE_ATTRIBUTES", "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "child test failed:\n{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // An obsolete --exact filter must not silently skip the regression.
        assert!(stdout.contains("1 passed; 0 failed"), "{stdout}");
    }

    #[test]
    fn merge_configuration_is_isolated_without_changing_worktree_indexes() {
        for object_format in ["sha1", "sha256"] {
            let dir = fs_utils::Utf8TempDir::new().unwrap();
            fs_err::create_dir(dir.path().join("main")).unwrap();
            let repo = init_repo(&dir.path().join("main"), object_format);
            commit_file(&repo, "a\n");
            let changed = commit_file(&repo, "b\n");
            let released = commit_file(&repo, "c\n");
            if std::env::var_os("RELEASE_PLZ_TEST_GLOBAL_MERGE_ATTRIBUTES").is_some() {
                // Confirm the child really loads the global rule in ordinary Git.
                assert_eq!(
                    repo.git(&["check-attr", "merge", "--", "file"]).unwrap(),
                    "file: merge: union"
                );
            }
            fs_err::write(repo.directory().join(".gitattributes"), "* merge=union\n").unwrap();
            repo.add_all_and_commit("merge attributes").unwrap();
            repo.git(&["config", "merge.default", "union"]).unwrap();
            let attributes_path = repo.directory().join(".git/info/attributes");
            fs_err::write(&attributes_path, "* merge=union\n").unwrap();
            let config_path = repo.directory().join(".git/config");
            let config_before = fs_err::read(&config_path).unwrap();
            let linked = dir.path().join("linked");
            repo.git(&["worktree", "add", "--detach", linked.as_str()])
                .unwrap();

            for path in [repo.directory(), &linked] {
                let source = Repo::new(path).unwrap();
                let index_path = source
                    .git(&["rev-parse", "--path-format=absolute", "--git-path", "index"])
                    .unwrap();
                let index_before = fs_err::read(&index_path).unwrap();
                let head_before = source.current_commit_hash().unwrap();
                let objects_before = source.git(&["count-objects", "-v"]).unwrap();
                let contents_before = fs_err::read(path.join("file")).unwrap();
                let changes = replay(&source, &changed, &released, None);

                assert!(changes.survives(&changed).unwrap());
                assert_eq!(fs_err::read(&index_path).unwrap(), index_before);
                assert_eq!(source.current_commit_hash().unwrap(), head_before);
                assert_eq!(
                    source.git(&["count-objects", "-v"]).unwrap(),
                    objects_before
                );
                assert_eq!(fs_err::read(path.join("file")).unwrap(), contents_before);
            }
            assert_eq!(fs_err::read(&config_path).unwrap(), config_before);
            assert_eq!(
                fs_err::read_to_string(attributes_path).unwrap(),
                "* merge=union\n"
            );
        }
    }

    #[test]
    fn release_conflicts_do_not_prove_partly_released_or_binary_changes_absent() {
        for object_format in ["sha1", "sha256"] {
            for (base, changed, released) in [
                (
                    "a\n1\n2\n3\n4\n5\n6\n7\n8\n9\nx\n",
                    "b\n1\n2\n3\n4\n5\n6\n7\n8\n9\ny\n",
                    "c\n1\n2\n3\n4\n5\n6\n7\n8\n9\ny\n",
                ),
                ("a\0", "b\0", "c\0"),
            ] {
                let dir = fs_utils::Utf8TempDir::new().unwrap();
                let repo = init_repo(dir.path(), object_format);
                commit_file(&repo, base);
                let changed = commit_file(&repo, changed);
                let released = commit_file(&repo, released);
                let changes = replay(&repo, &changed, &released, None);
                // A nonconflicting hunk still undoes a released change. Binary
                // conflicts cannot establish absence by choosing the release's bytes.
                assert!(!changes.survives(&changed).unwrap());
            }
        }
    }

    #[test]
    fn a_root_commit_can_be_replayed() {
        for object_format in ["sha1", "sha256"] {
            let dir = fs_utils::Utf8TempDir::new().unwrap();
            let repo = init_repo(dir.path(), object_format);
            // Repo::init creates a root commit containing README.md.
            let root = repo.current_commit_hash().unwrap();
            repo.git(&["rm", "README.md"]).unwrap();
            repo.add_all_and_commit("remove root's file").unwrap();
            let released = repo.current_commit_hash().unwrap();
            let changes = replay(&repo, &root, &released, None);
            assert!(changes.survives(&root).unwrap());
        }
    }

    #[test]
    fn a_merge_commit_is_replayed_relative_to_its_first_parent() {
        for object_format in ["sha1", "sha256"] {
            let dir = fs_utils::Utf8TempDir::new().unwrap();
            let repo = init_repo(dir.path(), object_format);
            fs_err::write(repo.directory().join("file"), "a\n").unwrap();
            repo.add_all_and_commit("base").unwrap();
            repo.git(&["checkout", "-b", "feature"]).unwrap();
            fs_err::write(repo.directory().join("file"), "b\n").unwrap();
            repo.add_all_and_commit("change file").unwrap();
            repo.checkout_head().unwrap();
            fs_err::write(repo.directory().join("unrelated"), "mainline\n").unwrap();
            repo.add_all_and_commit("mainline").unwrap();
            repo.git(&["merge", "--no-ff", "-m", "merge feature", "feature"])
                .unwrap();
            let changed = repo.current_commit_hash().unwrap();
            fs_err::write(repo.directory().join("file"), "a\n").unwrap();
            repo.add_all_and_commit("revert merged change").unwrap();
            let released = repo.current_commit_hash().unwrap();
            let changes = replay(
                &repo,
                &changed,
                &released,
                Some(HashSet::from([Utf8PathBuf::from("file")])),
            );
            // The first parent has the old file. Reverting relative to the
            // second parent would only remove the unrelated mainline file.
            assert!(changes.survives(&changed).unwrap());
        }
    }

    #[test]
    fn directory_rename_conflicts_include_the_original_packaged_path() {
        for object_format in ["sha1", "sha256"] {
            let dir = fs_utils::Utf8TempDir::new().unwrap();
            let repo = init_repo(dir.path(), object_format);
            fs_err::create_dir(repo.directory().join("old")).unwrap();
            fs_err::write(repo.directory().join("old/a"), "unchanged\n").unwrap();
            fs_err::write(repo.directory().join("old/b"), "removed\n").unwrap();
            repo.add_all_and_commit("add files").unwrap();
            let parent = repo.current_commit_hash().unwrap();
            repo.git(&["rm", "old/b"]).unwrap();
            repo.add_all_and_commit("remove packaged file").unwrap();
            let changed = repo.current_commit_hash().unwrap();
            repo.git(&["mv", "old", "new"]).unwrap();
            repo.add_all_and_commit("rename directory").unwrap();
            let target = repo.current_commit_hash().unwrap();
            let changes = replay(
                &repo,
                &target,
                &target,
                Some(HashSet::from([Utf8PathBuf::from("old/b")])),
            );
            // Undoing the deletion suggests new/b, but only the original old/b
            // is packaged. Git's structural conflict still affects this package.
            assert!(
                changes
                    .undo_changes_package(&changed, &parent, &target, false)
                    .unwrap()
            );
        }
    }
}
