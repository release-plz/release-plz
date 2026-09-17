use super::*;
use crate::{
    command::update::UpdateConfig,
    test_utils::{generate_lockfile, write_package},
};

const PACKAGE: &str = "history-test";

struct History {
    repo: Repo,
    registry: Repo,
    _local_dir: tempfile::TempDir,
    _registry_dir: tempfile::TempDir,
}

impl History {
    fn new() -> Self {
        Self::with_packages(|root| write_package(root, PACKAGE, "0.1.0", ""))
    }

    fn with_packages(write_packages: impl Fn(&Utf8Path)) -> Self {
        let local_dir = tempfile::tempdir().unwrap();
        let registry_dir = tempfile::tempdir().unwrap();
        // Resolve symlinks (such as macOS's /var) so metadata and project paths agree.
        let repo = Repo::init(canonicalize(&local_dir));
        let registry = Repo::init(canonicalize(&registry_dir));
        for repo in [&repo, &registry] {
            // Keep checked-out files byte-identical to the LF-only registry fixtures.
            repo.git(&["config", "core.autocrlf", "false"]).unwrap();
            write_packages(repo.directory());
            fs_err::write(repo.directory().join(".gitignore"), "/target\n").unwrap();
            generate_lockfile(repo.directory());
            repo.add_all_and_commit("chore: published baseline")
                .unwrap();
        }
        Self {
            repo,
            registry,
            _local_dir: local_dir,
            _registry_dir: registry_dir,
        }
    }

    fn write_commit(&self, path: &str, contents: &str, message: &str) -> String {
        fs_err::write(self.repo.directory().join(path), contents).unwrap();
        self.repo.add_all_and_commit(message).unwrap();
        self.repo.current_commit_hash().unwrap()
    }

    /// Like [`Self::write_commit`], but with an explicit author and committer date,
    /// so the test controls where the commit lands in the date-ordered walk.
    fn write_commit_at(&self, path: &str, contents: &str, message: &str, date: &str) -> String {
        fs_err::write(self.repo.directory().join(path), contents).unwrap();
        self.repo.git(&["add", "."]).unwrap();
        self.repo.git_at(&["commit", "-m", message], date).unwrap();
        self.repo.current_commit_hash().unwrap()
    }

    /// Ignore a revert of the current change, then import a sibling change
    /// through the reverted branch so that its equal snapshot is also visited.
    fn merge_ignored_revert(&self, path: &str, contents: Option<&str>) -> String {
        self.merge_ignored_change("src/fix.rs", |root| {
            let path = root.join(path);
            match contents {
                Some(contents) => fs_err::write(path, contents).unwrap(),
                None => fs_err::remove_file(path).unwrap(),
            }
        })
    }

    fn merge_ignored_change(&self, sibling_path: &str, revert: impl FnOnce(&Utf8Path)) -> String {
        self.repo.git(&["checkout", "-b", "equal"]).unwrap();
        revert(self.repo.directory());
        self.repo
            .add_all_and_commit("revert: breaking change")
            .unwrap();
        self.repo.checkout_head().unwrap();
        self.repo
            .git(&[
                "merge",
                "--no-ff",
                "-s",
                "ours",
                "-m",
                "merge equal",
                "equal",
            ])
            .unwrap();
        self.repo.git(&["checkout", "equal"]).unwrap();
        let sibling = self.write_commit(sibling_path, "", "fix: sibling");
        self.repo.checkout_head().unwrap();
        self.repo
            .git(&["merge", "--no-ff", "-m", "merge sibling", "equal"])
            .unwrap();
        sibling
    }

    /// Two sibling branches off the current commit, each merged back with a
    /// `--no-ff` merge. Returns the baseline and the two sibling commits.
    fn two_merged_siblings(&self) -> (String, String, String) {
        let repo = &self.repo;
        let baseline = repo.current_commit_hash().unwrap();
        repo.git(&["checkout", "-b", "one"]).unwrap();
        let one = self.write_commit("src/one.rs", "", "fix: sibling one");
        repo.git(&["checkout", "-b", "two", &baseline]).unwrap();
        let two = self.write_commit("src/two.rs", "", "fix: sibling two");
        repo.checkout_head().unwrap();
        for branch in ["one", "two"] {
            repo.git(&["merge", "--no-ff", "-m", "merge sibling", branch])
                .unwrap();
        }
        (baseline, one, two)
    }

    /// A feature branch discarded by a "keep mine" merge, then merged again with a
    /// `--no-ff` merge that makes its commits reachable from HEAD. `discarded_date`
    /// places the discarded commit relative to the equal snapshot in the date-ordered
    /// walk. Returns the discarded, the equal and the unreleased commit.
    fn feature_discarded_by_a_keep_mine_merge(
        &self,
        discarded_date: &str,
    ) -> (String, String, String) {
        let repo = &self.repo;
        repo.git(&["checkout", "-b", "feature"]).unwrap();
        let discarded = self.write_commit_at(
            "src/feature.rs",
            "",
            "feat: discarded by the merge",
            discarded_date,
        );
        repo.checkout_head().unwrap();
        self.write_commit_at(
            "src/lib.rs",
            "pub fn temporary() {}\n",
            "feat: temporary",
            "2000-01-01T00:00:00 +0000",
        );
        // "Keep mine": the merge commit has the same tree as its first parent, which is
        // what makes git prune the feature branch from walks rooted after it.
        repo.git_at(
            &["merge", "-s", "ours", "-m", "merge feature", "feature"],
            "2000-01-02T00:00:00 +0000",
        )
        .unwrap();
        // Back to the released tree, so this commit is the equal snapshot.
        let equal = self.write_commit_at(
            "src/lib.rs",
            "",
            "revert: temporary",
            "2000-01-03T00:00:00 +0000",
        );
        // A second merge of the same branch makes the discarded commit reachable again
        // from HEAD, this time through a merge that git doesn't simplify away.
        repo.git(&["checkout", "feature"]).unwrap();
        let unreleased = self.write_commit_at(
            "src/feature2.rs",
            "",
            "feat: unreleased",
            "2000-01-06T00:00:00 +0000",
        );
        repo.checkout_head().unwrap();
        repo.git_at(
            &["merge", "--no-ff", "-m", "merge feature again", "feature"],
            "2000-01-07T00:00:00 +0000",
        )
        .unwrap();
        (discarded, equal, unreleased)
    }

    fn diff(&self, published_at: Option<&str>) -> Diff {
        let metadata =
            cargo_utils::get_manifest_metadata(&self.registry.directory().join(CARGO_TOML))
                .unwrap();
        let package = cargo_utils::workspace_package(&metadata, PACKAGE).unwrap();
        self.diff_with(
            Some(RegistryPackage::new(
                package.clone(),
                published_at.map(str::to_owned),
            )),
            None,
        )
    }

    fn diff_with(&self, published: Option<RegistryPackage>, limit: Option<u32>) -> Diff {
        self.diff_configured(published, limit, |request| request)
    }

    /// Like [`Self::diff_with`], but with a chance to change the update request.
    fn diff_configured(
        &self,
        published: Option<RegistryPackage>,
        limit: Option<u32>,
        configure: impl FnOnce(UpdateRequest) -> UpdateRequest,
    ) -> Diff {
        let tip = self.repo.current_commit_hash().unwrap();
        let diff = self.try_diff_with(published, limit, configure).unwrap();
        assert_eq!(self.repo.current_commit_hash().unwrap(), tip);
        diff
    }

    fn try_diff_with(
        &self,
        published: Option<RegistryPackage>,
        limit: Option<u32>,
        configure: impl FnOnce(UpdateRequest) -> UpdateRequest,
    ) -> anyhow::Result<Diff> {
        let metadata =
            cargo_utils::get_manifest_metadata(&self.repo.directory().join(CARGO_TOML)).unwrap();
        let package = cargo_utils::workspace_package(&metadata, PACKAGE)
            .unwrap()
            .clone();
        let request = configure(
            UpdateRequest::new(metadata.clone())
                .unwrap()
                .with_max_analyze_commits(limit),
        );
        let project = Project::new(
            request.local_manifest(),
            None,
            &HashSet::new(),
            &metadata,
            &request,
        )
        .unwrap();
        let registry_packages = PackagesCollection::default().with_packages(
            published
                .into_iter()
                .map(|p| (p.package.name.to_string(), p))
                .collect(),
        );
        Updater {
            project: &project,
            req: &request,
        }
        .get_diff(&package, &registry_packages, &self.repo)
    }
}

fn canonicalize(directory: &tempfile::TempDir) -> Utf8PathBuf {
    let path = fs_utils::to_utf8_path(directory.path()).unwrap();
    fs_utils::canonicalize_utf8(path).unwrap()
}

fn commit_ids(diff: &Diff) -> HashSet<&str> {
    diff.commits
        .iter()
        .map(|commit| commit.id.as_str())
        .collect()
}

const BASE_API: &str = "pub fn api() {}\n\n\n\n\n\npub fn stable() {}\n";
const BREAKING_API: &str = "pub fn api(_: bool) {}\n\n\n\n\n\npub fn stable() {}\n";

fn api_history() -> History {
    History::with_packages(|root| {
        write_package(root, PACKAGE, "0.1.0", "");
        fs_err::write(root.join("src/lib.rs"), BASE_API).unwrap();
    })
}

fn assert_next_version(diff: &Diff, expected: &Version) {
    assert_eq!(
        Version::new(0, 1, 0).next_from_diff(diff, next_version::VersionUpdater::default()),
        *expected
    );
}

#[test]
fn an_ignored_revert_does_not_hide_surviving_sequential_api_changes() {
    for sequential in [false, true] {
        for skew in [false, true] {
            for boundary in ["equality", "tag", "unrelated published commit"] {
                let history = api_history();
                let repo = &history.repo;
                let published_at = match boundary {
                    "tag" => {
                        repo.tag_lightweight("v0.1.0").unwrap();
                        None
                    }
                    "unrelated published commit" => {
                        repo.git(&["checkout", "--orphan", "published"]).unwrap();
                        // The manifest matches the release, but this unrelated
                        // history has no source targets and cannot be packaged.
                        fs_err::remove_dir_all(repo.directory().join("src")).unwrap();
                        repo.add_all_and_commit("chore: unrelated published snapshot")
                            .unwrap();
                        let commit = repo.current_commit_hash().unwrap();
                        repo.checkout_head().unwrap();
                        Some(commit)
                    }
                    _ => None,
                };
                let implementation = if sequential {
                    BASE_API.replace("api() {}", "api() { /* implementation */ }")
                } else {
                    BASE_API.to_owned()
                };
                let breaking_api = implementation.replace("api()", "api(_: bool)");
                let implementation_commit = sequential.then(|| {
                    // Package equality ignores the root lockfile even when its
                    // bytes differ between the tag and later equal snapshot.
                    let lockfile = repo.directory().join("Cargo.lock");
                    let contents = fs_err::read_to_string(&lockfile).unwrap();
                    fs_err::write(lockfile, format!("{contents}# preparation\n")).unwrap();
                    history.write_commit_at(
                        "src/lib.rs",
                        &implementation,
                        "chore: modify implementation",
                        "2000-01-04T00:00:00 +0000",
                    )
                });
                let breaking = history.write_commit_at(
                    "src/lib.rs",
                    &breaking_api,
                    "feat!: breaking API",
                    "2000-01-05T00:00:00 +0000",
                );
                repo.git(&["checkout", "-b", "equal"]).unwrap();
                history.write_commit_at(
                    "src/lib.rs",
                    BASE_API,
                    "revert: API changes",
                    if skew {
                        "2000-01-02T00:00:00 +0000"
                    } else {
                        "2000-01-06T00:00:00 +0000"
                    },
                );
                repo.checkout_head().unwrap();
                repo.git_at(
                    &[
                        "merge",
                        "--no-ff",
                        "-s",
                        "ours",
                        "-m",
                        "merge equal",
                        "equal",
                    ],
                    "2000-01-09T00:00:00 +0000",
                )
                .unwrap();
                repo.git(&["checkout", "equal"]).unwrap();
                let sibling = history.write_commit_at(
                    "src/lib.rs",
                    &format!("{BASE_API}pub fn extra() {{}}\n"),
                    "fix: sibling",
                    if skew {
                        "2000-01-03T00:00:00 +0000"
                    } else {
                        "2000-01-07T00:00:00 +0000"
                    },
                );
                repo.checkout_head().unwrap();
                repo.git_at(
                    &["merge", "--no-ff", "-m", "merge sibling", "equal"],
                    "2000-01-10T00:00:00 +0000",
                )
                .unwrap();
                let merge = repo.current_commit_hash().unwrap();
                assert_eq!(
                    fs_err::read_to_string(repo.directory().join("src/lib.rs")).unwrap(),
                    format!("{breaking_api}pub fn extra() {{}}\n")
                );
                let diff = history.diff(published_at.as_deref());
                assert_next_version(&diff, &Version::new(0, 2, 0));
                let mut expected =
                    HashSet::from([breaking.as_str(), sibling.as_str(), merge.as_str()]);
                expected.extend(implementation_commit.as_deref());
                assert_eq!(
                    commit_ids(&diff),
                    expected,
                    "sequential={sequential}, skew={skew}, boundary={boundary}: {:?}",
                    diff.commits
                );
            }
        }
    }
}

#[test]
fn a_discarded_change_stays_excluded_when_a_sibling_changes_the_same_file() {
    for sequential in [false, true] {
        for discarded_date in ["2000-01-02T00:00:00 +0000", "2000-01-08T00:00:00 +0000"] {
            let history = api_history();
            let repo = &history.repo;
            repo.git(&["checkout", "-b", "feature"]).unwrap();
            let implementation = if sequential {
                BASE_API.replace("api() {}", "api() { /* implementation */ }")
            } else {
                BASE_API.to_owned()
            };
            let breaking_api = implementation.replace("api()", "api(_: bool)");
            if sequential {
                history.write_commit_at(
                    "src/lib.rs",
                    &implementation,
                    "chore: modify implementation",
                    "2000-01-01T00:00:00 +0000",
                );
            }
            history.write_commit_at(
                "src/lib.rs",
                &breaking_api,
                "feat!: discarded breaking API",
                discarded_date,
            );
            repo.checkout_head().unwrap();
            history.write_commit_at(
                "src/lib.rs",
                &format!("{BASE_API}// temporary\n"),
                "chore: temporary",
                "2000-01-03T00:00:00 +0000",
            );
            repo.git_at(
                &["merge", "-s", "ours", "-m", "merge feature", "feature"],
                "2000-01-04T00:00:00 +0000",
            )
            .unwrap();
            history.write_commit_at(
                "src/lib.rs",
                BASE_API,
                "revert: temporary",
                "2000-01-05T00:00:00 +0000",
            );
            repo.git(&["checkout", "feature"]).unwrap();
            let sibling = history.write_commit_at(
                "src/lib.rs",
                &format!("{breaking_api}pub fn extra() {{}}\n"),
                "fix: sibling",
                "2000-01-09T00:00:00 +0000",
            );
            repo.checkout_head().unwrap();
            repo.git_at(
                &["merge", "--no-ff", "-m", "merge feature again", "feature"],
                "2000-01-10T00:00:00 +0000",
            )
            .unwrap();
            let merge = repo.current_commit_hash().unwrap();
            assert_eq!(
                fs_err::read_to_string(repo.directory().join("src/lib.rs")).unwrap(),
                format!("{BASE_API}pub fn extra() {{}}\n")
            );
            let diff = history.diff(None);
            assert_eq!(
                commit_ids(&diff),
                HashSet::from([sibling.as_str(), merge.as_str()]),
                "sequential={sequential}, discarded_date={discarded_date}: {:?}",
                diff.commits
            );
            assert_next_version(&diff, &Version::new(0, 1, 1));
        }
    }
}

#[test]
fn later_same_line_edits_preserve_only_surviving_breaking_change_markers() {
    for breaking_survives in [false, true] {
        let history = api_history();
        let implementation = BASE_API.replace("api() {}", "api() { /* implementation */ }");
        let implementation_commit = history.write_commit(
            "src/lib.rs",
            &implementation,
            "chore: modify implementation",
        );
        let breaking = history.write_commit(
            "src/lib.rs",
            &implementation.replace("api()", "api(_: bool)"),
            "feat!: breaking API",
        );
        let sibling = history.merge_ignored_revert("src/lib.rs", Some(BASE_API));
        // Evolve the implementation on the same line, either retaining the breaking
        // signature or restoring the original. The line-based inverse patch now
        // conflicts in both cases, although only one still needs a breaking bump.
        let evolved = implementation
            .replace("/* implementation */", "/* implementation */ /* sibling */")
            .replace(
                "api()",
                if breaking_survives {
                    "api(_: bool)"
                } else {
                    "api()"
                },
            );
        let evolved_commit =
            history.write_commit("src/lib.rs", &evolved, "fix: evolve implementation");
        let diff = history.diff(None);
        assert_next_version(
            &diff,
            &if breaking_survives {
                Version::new(0, 2, 0)
            } else {
                Version::new(0, 1, 1)
            },
        );
        let mut expected = HashSet::from([
            implementation_commit.as_str(),
            sibling.as_str(),
            evolved_commit.as_str(),
        ]);
        if breaking_survives {
            expected.insert(breaking.as_str());
        }
        assert_eq!(
            commit_ids(&diff),
            expected,
            "breaking_survives={breaking_survives}: {:?}",
            diff.commits
        );
    }
}

#[test]
fn an_evolved_released_api_does_not_repeat_its_breaking_change_marker() {
    let history = api_history();
    let repo = &history.repo;
    let published_api = BREAKING_API.replace(
        "api(_: bool) {}",
        "api(_: bool) { /* published implementation */ }",
    );
    fs_err::write(
        history.registry.directory().join("src/lib.rs"),
        &published_api,
    )
    .unwrap();
    history
        .registry
        .add_all_and_commit("published release")
        .unwrap();
    history.write_commit("src/lib.rs", BREAKING_API, "feat!: released breaking API");
    repo.git(&["checkout", "-b", "equal"]).unwrap();
    history.write_commit(
        "src/lib.rs",
        &published_api,
        "chore: published implementation",
    );
    repo.checkout_head().unwrap();
    repo.git(&[
        "merge",
        "--no-ff",
        "-s",
        "ours",
        "-m",
        "merge equal",
        "equal",
    ])
    .unwrap();
    repo.git(&["checkout", "equal"]).unwrap();
    let sibling = history.write_commit(
        "src/lib.rs",
        &format!("{published_api}pub fn extra() {{}}\n"),
        "fix: sibling",
    );
    repo.checkout_head().unwrap();
    repo.git(&["merge", "--no-ff", "-m", "merge sibling", "equal"])
        .unwrap();
    let merge = repo.current_commit_hash().unwrap();
    assert_eq!(
        fs_err::read_to_string(repo.directory().join("src/lib.rs")).unwrap(),
        format!("{BREAKING_API}pub fn extra() {{}}\n")
    );
    let diff = history.diff(None);
    assert_eq!(
        commit_ids(&diff),
        HashSet::from([sibling.as_str(), merge.as_str()]),
        "{:?}",
        diff.commits
    );
    assert_next_version(&diff, &Version::new(0, 1, 1));
}

#[test]
fn conflict_resolution_can_preserve_a_change_reverted_on_another_branch() {
    let history = api_history();
    let repo = &history.repo;
    let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    repo.git(&["checkout", "-b", "equal"]).unwrap();
    history.write_commit("src/lib.rs", BASE_API, "revert: breaking API");
    repo.checkout_head().unwrap();
    history.write_commit(
        "src/lib.rs",
        &BASE_API.replace("api()", "api(_: u8)"),
        "chore: prepare merge",
    );
    assert!(
        repo.git(&["merge", "--no-ff", "--no-commit", "equal"])
            .is_err()
    );
    history.write_commit("src/lib.rs", BREAKING_API, "merge resolved");
    repo.git(&["checkout", "equal"]).unwrap();
    history.write_commit(
        "src/lib.rs",
        &format!("{BASE_API}pub fn extra() {{}}\n"),
        "fix: sibling",
    );
    repo.checkout_head().unwrap();
    repo.git(&["merge", "--no-ff", "-m", "merge sibling", "equal"])
        .unwrap();
    assert_eq!(
        fs_err::read_to_string(repo.directory().join("src/lib.rs")).unwrap(),
        format!("{BREAKING_API}pub fn extra() {{}}\n")
    );
    let diff = history.diff(None);
    assert!(commit_ids(&diff).contains(breaking.as_str()));
    assert_next_version(&diff, &Version::new(0, 2, 0));
}

#[test]
fn ignored_file_changes_do_not_hide_a_retained_package_change() {
    for ignored in [
        "ignored.txt",
        "Cargo.lock",
        "src/Cargo.lock",
        "src/Cargo.toml.orig",
    ] {
        let history = History::with_packages(|root| {
            write_package(root, PACKAGE, "0.1.0", "exclude = [\"ignored.txt\"]\n");
            fs_err::write(root.join("src/lib.rs"), BASE_API).unwrap();
            fs_err::write(root.join("ignored.txt"), "original\n").unwrap();
            for nested in ["src/Cargo.lock", "src/Cargo.toml.orig"] {
                fs_err::write(root.join(nested), "original\n").unwrap();
            }
        });
        let path = history.repo.directory().join(ignored);
        let old = fs_err::read_to_string(&path).unwrap();
        fs_err::write(path, format!("{old}# changed\n")).unwrap();
        let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
        let sibling = history.merge_ignored_revert("src/lib.rs", Some(BASE_API));
        let diff = history.diff(None);
        assert_eq!(
            commit_ids(&diff),
            HashSet::from([breaking.as_str(), sibling.as_str()]),
            "ignored={ignored}"
        );
        assert_next_version(&diff, &Version::new(0, 2, 0));
    }
}

#[cfg(unix)]
#[test]
fn executable_bit_changes_do_not_hide_a_retained_package_change() {
    use std::os::unix::fs::PermissionsExt;

    for sequential in [false, true] {
        let history = api_history();
        history
            .repo
            .git(&["config", "core.filemode", "true"])
            .unwrap();
        let implementation = if sequential {
            let implementation = BASE_API.replace("api() {}", "api() { /* implementation */ }");
            history.write_commit("src/lib.rs", &implementation, "chore: implementation");
            implementation
        } else {
            BASE_API.to_owned()
        };
        fs_err::set_permissions(
            history.repo.directory().join("src/lib.rs"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        let breaking = history.write_commit(
            "src/lib.rs",
            &implementation.replace("api()", "api(_: bool)"),
            "feat!: breaking API and set executable bit",
        );
        history.merge_ignored_revert("src/lib.rs", Some(BASE_API));
        let diff = history.diff(None);
        assert!(
            commit_ids(&diff).contains(breaking.as_str()),
            "sequential={sequential}"
        );
        assert_next_version(&diff, &Version::new(0, 2, 0));

        // Keeping only the executable bit must not retain the breaking marker.
        history.write_commit("src/lib.rs", BASE_API, "fix: restore API");
        let diff = history.diff(None);
        assert!(
            !commit_ids(&diff).contains(breaking.as_str()),
            "sequential={sequential}"
        );
        assert_next_version(&diff, &Version::new(0, 1, 1));
    }
}

#[test]
fn materialized_symlink_files_keep_their_breaking_change_marker() {
    for (path, sequential) in [
        ("src/link.txt", false),
        ("src/link.txt", true),
        ("API.md", false),
        ("API.md", true),
    ] {
        let history = History::with_packages(|root| {
            let readme = if path == "API.md" {
                "readme = \"API.md\"\n"
            } else {
                ""
            };
            write_package(root, PACKAGE, "0.1.0", readme);
            fs_err::write(root.join(path), "old-target.txt").unwrap();
            for target in [
                "old-target.txt",
                "new-target.txt",
                "old-target.txt-extra",
                "new-target.txt-extra",
            ] {
                fs_err::write(
                    root.join(path).parent().unwrap().join(target),
                    "# same contents\n",
                )
                .unwrap();
            }
        });
        for repo in [&history.repo, &history.registry] {
            repo.git(&["config", "core.symlinks", "false"]).unwrap();
            let blob = repo.git(&["hash-object", "-w", "--", path]).unwrap();
            repo.git(&[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("120000,{blob},{path}"),
            ])
            .unwrap();
            repo.git(&["commit", "-m", "chore: materialized link baseline"])
                .unwrap();
            assert!(!repo.directory().join(path).is_symlink());
            assert!(
                repo.git(&["ls-files", "--stage", "--", path])
                    .unwrap()
                    .starts_with("120000 ")
            );
        }
        let contents = if sequential {
            history.write_commit(path, "old-target.txt-extra", "chore: pointer suffix");
            "new-target.txt-extra"
        } else {
            "new-target.txt"
        };
        let breaking = history.write_commit(path, contents, "feat!: pointer format");
        let sibling = history.merge_ignored_revert(path, Some("old-target.txt"));
        let diff = history.diff(None);
        assert!(
            HashSet::from([breaking.as_str(), sibling.as_str()]).is_subset(&commit_ids(&diff)),
            "path={path}, sequential={sequential}"
        );
        assert_next_version(&diff, &Version::new(0, 2, 0));

        history.write_commit(path, "old-target.txt", "fix: restore pointer");
        let diff = history.diff(None);
        assert!(
            !commit_ids(&diff).contains(breaking.as_str()),
            "path={path}, sequential={sequential}"
        );
        assert_next_version(&diff, &Version::new(0, 1, 1));
    }
}

#[cfg(unix)]
#[test]
fn symlink_target_changes_do_not_hide_a_retained_package_change() {
    use std::os::unix::fs::symlink;

    for (was_symlink, conflicting) in [(false, false), (true, false), (true, true)] {
        let history = History::with_packages(|root| {
            write_package(root, PACKAGE, "0.1.0", "");
            fs_err::write(root.join("src/lib.rs"), BASE_API).unwrap();
            for target in ["a.txt", "b.txt", "c.txt"] {
                fs_err::write(root.join("src").join(target), target).unwrap();
            }
            let link = root.join("src/link.txt");
            if was_symlink {
                symlink("a.txt", link).unwrap();
            } else {
                fs_err::write(link, "original\n").unwrap();
            }
        });
        let link = history.repo.directory().join("src/link.txt");
        fs_err::remove_file(&link).unwrap();
        symlink("b.txt", &link).unwrap();
        let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
        if conflicting {
            // Undoing a -> b at c conflicts even though equality ignores the link.
            fs_err::remove_file(&link).unwrap();
            symlink("c.txt", &link).unwrap();
            history
                .repo
                .add_all_and_commit("chore: retarget link")
                .unwrap();
        }
        history.merge_ignored_revert("src/lib.rs", Some(BASE_API));
        let diff = history.diff(None);
        assert!(
            commit_ids(&diff).contains(breaking.as_str()),
            "was_symlink={was_symlink}, conflicting={conflicting}"
        );
        assert_next_version(&diff, &Version::new(0, 2, 0));

        history.write_commit("src/lib.rs", BASE_API, "fix: restore API");
        let diff = history.diff(None);
        assert!(
            !commit_ids(&diff).contains(breaking.as_str()),
            "was_symlink={was_symlink}, conflicting={conflicting}"
        );
        assert_next_version(&diff, &Version::new(0, 1, 1));
    }
}

#[cfg(unix)]
#[test]
fn symlink_presence_changes_keep_their_breaking_change_marker() {
    use std::os::unix::fs::symlink;

    for added in [false, true] {
        let history = History::with_packages(|root| {
            write_package(root, PACKAGE, "0.1.0", "");
            fs_err::write(root.join("src/target.txt"), "fixture\n").unwrap();
            if !added {
                symlink("target.txt", root.join("src/link.txt")).unwrap();
            }
        });
        let link = history.repo.directory().join("src/link.txt");
        if added {
            symlink("target.txt", &link).unwrap();
        } else {
            fs_err::remove_file(&link).unwrap();
        }
        // Include a regular packaged path in the commit; its contents are ignored.
        let lock = fs_err::read_to_string(history.repo.directory().join("Cargo.lock")).unwrap();
        let breaking = history.write_commit(
            "Cargo.lock",
            &format!("{lock}# changed\n"),
            "feat!: fixture paths",
        );
        let sibling = history.merge_ignored_change("src/fix.rs", |root| {
            let link = root.join("src/link.txt");
            if added {
                fs_err::remove_file(link).unwrap();
            } else {
                symlink("target.txt", link).unwrap();
            }
        });
        let diff = history.diff(None);
        assert_eq!(
            commit_ids(&diff),
            HashSet::from([breaking.as_str(), sibling.as_str()]),
            "added={added}"
        );
        assert_next_version(&diff, &Version::new(0, 2, 0));
    }
}

#[cfg(unix)]
#[test]
fn readme_symlink_changes_keep_their_breaking_change_marker() {
    use std::os::unix::fs::symlink;

    for (package_dir, readme_link) in [("", "API.md"), ("app", "API.md"), ("", "docs/current.md")] {
        let target_prefix = if readme_link == "API.md" { "" } else { "../" };
        let old_target = format!("{target_prefix}old.md");
        let new_target = format!("{target_prefix}new.md");
        let ignored = Utf8Path::new(package_dir).join("src/Cargo.lock");
        let history = History::with_packages(|root| {
            let readme = if package_dir.is_empty() {
                "API.md"
            } else {
                fs_err::write(root.join(CARGO_TOML), "[workspace]\nmembers = [\"app\"]\n").unwrap();
                "../API.md"
            };
            write_package(
                &root.join(package_dir),
                PACKAGE,
                "0.1.0",
                &format!("readme = {readme:?}\n"),
            );
            fs_err::write(root.join(&ignored), "original\n").unwrap();
            for target in ["old.md", "new.md"] {
                fs_err::write(root.join(target), target).unwrap();
            }
            if readme_link != "API.md" {
                fs_err::create_dir(root.join("docs")).unwrap();
                symlink(readme_link, root.join("API.md")).unwrap();
            }
            symlink(&old_target, root.join(readme_link)).unwrap();
        });
        let readme = history.repo.directory().join(readme_link);
        fs_err::remove_file(&readme).unwrap();
        symlink(&new_target, &readme).unwrap();
        let breaking = history.write_commit(ignored.as_str(), "changed\n", "feat!: documented API");
        let sibling = history.merge_ignored_change(
            Utf8Path::new(package_dir).join("src/fix.rs").as_str(),
            |root| {
                let readme = root.join(readme_link);
                fs_err::remove_file(&readme).unwrap();
                symlink(&old_target, readme).unwrap();
                // Keep the equal snapshot in the package's path-filtered history
                // even when its README link lives outside the package directory.
                fs_err::write(root.join(&ignored), "reverted\n").unwrap();
            },
        );
        let diff = history.diff(None);
        assert!(
            HashSet::from([breaking.as_str(), sibling.as_str()]).is_subset(&commit_ids(&diff)),
            "package_dir={package_dir}, readme_link={readme_link}: {:?}",
            diff.commits
        );
        assert_next_version(&diff, &Version::new(0, 2, 0));
    }
}

#[test]
fn sequential_readme_edits_keep_their_breaking_change_marker() {
    let history = History::with_packages(|root| {
        write_package(root, PACKAGE, "0.1.0", "readme = \"API.md\"\n");
        fs_err::write(root.join("API.md"), BASE_API).unwrap();
    });
    let implementation = BASE_API.replace("api() {}", "api() { /* implementation */ }");
    history.write_commit("API.md", &implementation, "chore: clarify documentation");
    let breaking = history.write_commit(
        "API.md",
        &implementation.replace("api()", "api(_: bool)"),
        "feat!: documented API",
    );
    history.merge_ignored_revert("API.md", Some(BASE_API));
    let diff = history.diff(None);
    assert!(commit_ids(&diff).contains(breaking.as_str()));
    assert_next_version(&diff, &Version::new(0, 2, 0));
}

#[cfg(unix)]
#[test]
fn equivalent_readme_symlinks_do_not_hide_a_retained_api_change() {
    use std::os::unix::fs::symlink;

    for conflicting in [false, true] {
        let history = History::with_packages(|root| {
            write_package(root, PACKAGE, "0.1.0", "readme = \"API.md\"\n");
            fs_err::write(root.join("src/lib.rs"), BASE_API).unwrap();
            for target in ["a.md", "b.md", "c.md"] {
                fs_err::write(root.join(target), "# API\n").unwrap();
            }
            fs_err::create_dir(root.join("docs")).unwrap();
            symlink("../a.md", root.join("docs/current.md")).unwrap();
            symlink("docs/current.md", root.join("API.md")).unwrap();
        });
        let readme = history.repo.directory().join("API.md");
        fs_err::remove_file(&readme).unwrap();
        symlink("./docs/../b.md", &readme).unwrap();
        let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
        if conflicting {
            // The inverse link edit conflicts, but every alias still reads the
            // same README bytes. Its spelling must not hide the API contribution.
            fs_err::remove_file(&readme).unwrap();
            symlink("c.md", &readme).unwrap();
            history
                .repo
                .add_all_and_commit("chore: another README alias")
                .unwrap();
        }
        history.merge_ignored_revert("src/lib.rs", Some(BASE_API));
        let diff = history.diff(None);
        assert!(
            commit_ids(&diff).contains(breaking.as_str()),
            "conflicting={conflicting}"
        );
        assert_next_version(&diff, &Version::new(0, 2, 0));

        history.write_commit("src/lib.rs", BASE_API, "fix: restore API");
        let diff = history.diff(None);
        assert!(
            !commit_ids(&diff).contains(breaking.as_str()),
            "conflicting={conflicting}"
        );
        assert_next_version(&diff, &Version::new(0, 1, 1));
    }
}

#[cfg(unix)]
#[test]
fn readme_parent_symlinks_are_not_collapsed_lexically() {
    use std::os::unix::fs::symlink;

    let history = History::with_packages(|root| {
        write_package(root, PACKAGE, "0.1.0", "readme = \"API.md\"\n");
        fs_err::create_dir(root.join("docs")).unwrap();
        fs_err::create_dir_all(root.join("alt/subdir")).unwrap();
        fs_err::write(root.join("old.md"), "old\n").unwrap();
        fs_err::write(root.join("docs/b.md"), "old\n").unwrap();
        fs_err::write(root.join("alt/b.md"), "new\n").unwrap();
        fs_err::write(root.join("alt/subdir/keep"), "").unwrap();
        symlink("../alt/subdir", root.join("docs/link")).unwrap();
        symlink("old.md", root.join("API.md")).unwrap();
    });
    let readme = history.repo.directory().join("API.md");
    fs_err::remove_file(&readme).unwrap();
    symlink("docs/link/../b.md", &readme).unwrap();
    // Following the directory link reaches alt/b.md, not docs/b.md.
    assert_eq!(fs_err::read_to_string(&readme).unwrap(), "new\n");
    let lock = fs_err::read_to_string(history.repo.directory().join("Cargo.lock")).unwrap();
    let breaking = history.write_commit(
        "Cargo.lock",
        &format!("{lock}# changed\n"),
        "feat!: documented API",
    );
    history.merge_ignored_change("src/fix.rs", |root| {
        let readme = root.join("API.md");
        fs_err::remove_file(&readme).unwrap();
        symlink("old.md", readme).unwrap();
    });
    let diff = history.diff(None);
    assert!(commit_ids(&diff).contains(breaking.as_str()));
    assert_next_version(&diff, &Version::new(0, 2, 0));
}

#[test]
fn nested_cargo_vcs_info_changes_keep_their_breaking_change_marker() {
    let path = "src/.cargo_vcs_info.json";
    let history = History::with_packages(|root| {
        write_package(root, PACKAGE, "0.1.0", "");
        fs_err::write(root.join(path), "{}\n").unwrap();
    });
    let breaking = history.write_commit(path, "{\"breaking\":true}\n", "feat!: fixture format");
    let sibling = history.merge_ignored_revert(path, Some("{}\n"));
    let diff = history.diff(None);
    assert_eq!(
        commit_ids(&diff),
        HashSet::from([breaking.as_str(), sibling.as_str()])
    );
    assert_next_version(&diff, &Version::new(0, 2, 0));
}

#[test]
fn nested_metadata_file_additions_keep_their_breaking_change_marker() {
    for path in ["src/Cargo.lock", "src/Cargo.toml.orig"] {
        let history = History::new();
        let breaking = history.write_commit(path, "fixture\n", "feat!: fixture format");
        let sibling = history.merge_ignored_revert(path, None);
        let diff = history.diff(None);
        assert_eq!(
            commit_ids(&diff),
            HashSet::from([breaking.as_str(), sibling.as_str()]),
            "path={path}"
        );
        assert_next_version(&diff, &Version::new(0, 2, 0));
    }
}

#[test]
fn a_retained_api_deletion_keeps_its_breaking_change_marker() {
    let history = api_history();
    let breaking = history.write_commit("src/lib.rs", "pub fn stable() {}\n", "feat!: remove API");
    let sibling = history.merge_ignored_revert("src/lib.rs", Some(BASE_API));
    let diff = history.diff(None);
    assert_eq!(
        commit_ids(&diff),
        HashSet::from([breaking.as_str(), sibling.as_str()])
    );
    assert_next_version(&diff, &Version::new(0, 2, 0));
}

#[test]
fn a_retained_change_can_move_to_a_different_file() {
    let history = api_history();
    let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    history.merge_ignored_revert("src/lib.rs", Some(BASE_API));
    history
        .repo
        .git(&["mv", "src/lib.rs", "src/api.rs"])
        .unwrap();
    history.write_commit(
        "src/lib.rs",
        "mod api;\npub use api::*;\n",
        "chore: move API",
    );
    let diff = history.diff(None);
    assert!(commit_ids(&diff).contains(breaking.as_str()));
    assert_next_version(&diff, &Version::new(0, 2, 0));
}

#[test]
fn sibling_commits_are_collected_with_tag_published_sha_or_equality_boundary() {
    let history = History::new();
    let repo = &history.repo;
    let (baseline, one, two) = history.two_merged_siblings();
    repo.git(&["tag", "v0.1.0", &baseline]).unwrap();
    let expected = HashSet::from([one.as_str(), two.as_str()]);
    assert_eq!(commit_ids(&history.diff(None)), expected);
    repo.git(&["tag", "-d", "v0.1.0"]).unwrap();
    for published_at in [Some(baseline.as_str()), None] {
        assert_eq!(commit_ids(&history.diff(published_at)), expected);
    }
}

#[test]
fn late_merge_keeps_mainline_changes_after_the_release() {
    let history = History::new();
    let repo = &history.repo;
    repo.git(&["checkout", "-b", "old-branch"]).unwrap();
    let branch = history.write_commit("src/branch.rs", "", "fix: old branch");
    repo.checkout_head().unwrap();
    history.write_commit("src/released.rs", "", "feat: already released");
    fs_err::write(history.registry.directory().join("src/released.rs"), "").unwrap();
    history
        .registry
        .add_all_and_commit("published release")
        .unwrap();
    repo.tag_lightweight("v0.1.0").unwrap();
    let mainline = history.write_commit("src/mainline.rs", "", "fix: mainline");
    repo.git(&["merge", "--no-ff", "-m", "merge old branch", "old-branch"])
        .unwrap();
    assert_eq!(
        commit_ids(&history.diff(None)),
        HashSet::from([branch.as_str(), mainline.as_str()])
    );
}

#[test]
fn final_revert_does_not_release_reverted_changes() {
    let history = History::new();
    history.repo.tag_lightweight("v0.1.0").unwrap();
    history.write_commit("src/lib.rs", "pub fn temporary() {}\n", "feat: temporary");
    history.write_commit("src/lib.rs", "", "revert: temporary");
    assert!(history.diff(None).commits.is_empty());
    history.repo.git(&["tag", "-d", "v0.1.0"]).unwrap();
    assert!(history.diff(None).commits.is_empty());
}

#[test]
fn equal_snapshot_excludes_its_ancestors_but_keeps_sibling_changes() {
    let history = History::new();
    let repo = &history.repo;
    repo.git(&["checkout", "-b", "branch"]).unwrap();
    history.write_commit("src/lib.rs", "pub fn temporary() {}\n", "feat: temporary");
    let equal = history.write_commit("src/lib.rs", "", "revert: temporary");
    let branch = history.write_commit("src/branch.rs", "", "fix: branch");
    repo.checkout_head().unwrap();
    // Date the sibling before the branch, so the walk reaches the equal snapshot
    // first: that's the order in which stopping there would lose the sibling.
    let sibling = history.write_commit_at(
        "src/sibling.rs",
        "",
        "fix: sibling",
        "2000-01-01T00:00:00 +0000",
    );
    repo.git(&["merge", "--no-ff", "-m", "merge branch", "branch"])
        .unwrap();
    // Exercise the order where stopping at the equal snapshot would lose its sibling.
    let order = repo
        .git(&["rev-list", "--date-order", "HEAD", "--", "."])
        .unwrap();
    assert!(order.find(&equal).unwrap() < order.find(&sibling).unwrap());
    assert_eq!(
        commit_ids(&history.diff(None)),
        HashSet::from([branch.as_str(), sibling.as_str()])
    );
}

/// Git's default history simplification prunes a merge's other parent when the
/// merge is TREESAME to one of them, so the commits reachable through that parent
/// are missing from a walk rooted at one of its descendants, even though they are
/// real ancestors of it. The pruning of an equal snapshot must not miss them:
/// otherwise a branch discarded by a "keep mine" merge resurfaces in the changelog
/// of every later release once a second merge makes it reachable again.
#[test]
fn a_merge_discarding_a_branch_still_prunes_it_with_the_equal_snapshot() {
    let history = History::new();
    let (discarded, equal, unreleased) =
        history.feature_discarded_by_a_keep_mine_merge("1999-12-31T00:00:00 +0000");
    assert!(
        history.repo.is_ancestor(&discarded, &equal),
        "the discarded commit must be a real ancestor of the equal snapshot"
    );
    // Exercise the order where the equal snapshot is visited before the commit it
    // has to prune.
    let order = history
        .repo
        .git(&["rev-list", "--date-order", "HEAD", "--", "."])
        .unwrap();
    assert!(
        order.find(&equal).unwrap() < order.find(&discarded).unwrap(),
        "{order}"
    );
    assert_eq!(
        commit_ids(&history.diff(None)),
        HashSet::from([unreleased.as_str()]),
        "an ancestor of the equal snapshot was released again"
    );
}

/// The mirror image of
/// [`a_merge_discarding_a_branch_still_prunes_it_with_the_equal_snapshot`]: here the
/// discarded commit is dated after the equal snapshot, so the walk reaches it first.
/// `--date-order` can't prevent that, because simplification severed the only edge
/// that connects the two, so pruning must not depend on the visit order.
#[test]
fn an_ancestor_visited_before_the_equal_snapshot_is_still_pruned() {
    let history = History::new();
    let (discarded, equal, unreleased) =
        history.feature_discarded_by_a_keep_mine_merge("2000-01-05T00:00:00 +0000");
    assert!(
        history.repo.is_ancestor(&discarded, &equal),
        "the discarded commit must be a real ancestor of the equal snapshot"
    );
    // Exercise the unfavourable order: the walk this simplifies exactly like the
    // diff's own one has to reach the discarded commit before the equal snapshot.
    let order = history
        .repo
        .git(&["rev-list", "--date-order", "HEAD", "--", "."])
        .unwrap();
    assert!(
        order.find(&discarded).unwrap() < order.find(&equal).unwrap(),
        "{order}"
    );
    assert_eq!(
        commit_ids(&history.diff(None)),
        HashSet::from([unreleased.as_str()]),
        "an ancestor of the equal snapshot was released again"
    );
}

#[test]
fn workspace_dependency_updates_are_detected_without_package_commits() {
    for update_lockfile in [false, true] {
        let history = History::with_packages(|root| {
            fs_err::write(
                root.join(CARGO_TOML),
                "[workspace]\nmembers = [\"app\", \"dep\"]\nresolver = \"2\"\n\
                 [workspace.dependencies]\nhistory-dependency = \"1\"\n\
                 [patch.crates-io]\nhistory-dependency = { path = \"dep\" }\n",
            )
            .unwrap();
            write_package(
                &root.join("app"),
                PACKAGE,
                "0.1.0",
                "[dependencies]\nhistory-dependency.workspace = true\n",
            );
            // Lockfile changes only trigger releases for executables.
            fs_err::write(root.join("app/src/main.rs"), "fn main() {}\n").unwrap();
            write_package(&root.join("dep"), "history-dependency", "1.0.0", "");
        });
        let repo = &history.repo;
        repo.tag_lightweight("history-test-v0.1.0").unwrap();
        assert!(history.diff(None).commits.is_empty());
        let (path, old, new, expected) = if update_lockfile {
            (
                "dep/Cargo.toml",
                "1.0.0",
                "1.0.1",
                "chore: update Cargo.lock dependencies",
            )
        } else {
            (
                "Cargo.toml",
                "history-dependency = \"1\"",
                "history-dependency = \">=1.0.0\"",
                "chore: update Cargo.toml dependencies",
            )
        };
        let manifest = repo.directory().join(path);
        let contents = fs_err::read_to_string(&manifest).unwrap();
        fs_err::write(manifest, contents.replace(old, new)).unwrap();
        generate_lockfile(repo.directory());
        repo.add_all_and_commit("chore: workspace dependencies")
            .unwrap();
        assert!(
            repo.git(&["rev-list", "history-test-v0.1.0..HEAD", "--", "app"])
                .unwrap()
                .is_empty()
        );
        let diff = history.diff(None);
        assert_eq!(diff.commits.len(), 1);
        assert_eq!(diff.commits[0].message, expected);
        assert_eq!(diff.commits[0].id, NO_COMMIT_ID);
    }
}

#[test]
fn first_release_respects_the_commit_limit() {
    let history = History::new();
    let baseline = history.repo.current_commit_hash().unwrap();
    // `Repo::init` commits a README before the package baseline.
    let readme = history
        .repo
        .git(&["rev-parse", &format!("{baseline}^")])
        .unwrap();
    let one = history.write_commit("src/one.rs", "", "fix: one");
    let two = history.write_commit("src/two.rs", "", "fix: two");
    assert_eq!(
        commit_ids(&history.diff_with(None, Some(1))),
        HashSet::from([two.as_str()])
    );
    assert_eq!(
        commit_ids(&history.diff_with(None, Some(2))),
        HashSet::from([one.as_str(), two.as_str()])
    );
    // Zero means no limit. The commits are collected newest first, which is the
    // order the changelog renders them in.
    let diff = history.diff_with(None, Some(0));
    assert_eq!(
        diff.commits
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        [
            two.as_str(),
            one.as_str(),
            baseline.as_str(),
            readme.as_str()
        ]
    );
}

/// The git tag bounds the history of packages that aren't in the registry too.
/// It's the only release boundary a `publish = false` or `git_only` package has:
/// without it, every release would repeat the whole history in its changelog.
#[test]
fn a_tag_bounds_the_history_of_a_package_that_is_not_published() {
    let history = History::new();
    history.write_commit("src/released.rs", "", "feat: released by the tag");
    history.repo.tag_lightweight("v0.1.0").unwrap();
    let unreleased = history.write_commit("src/unreleased.rs", "", "feat: after the tag");
    let diff = history.diff_configured(None, None, |request| {
        request.with_default_package_config(UpdateConfig {
            publish: false,
            ..UpdateConfig::default()
        })
    });
    assert_eq!(commit_ids(&diff), HashSet::from([unreleased.as_str()]));
}

#[test]
fn a_blocking_dirty_working_tree_hints_at_the_allow_dirty_option() {
    let history = History::new();
    history.write_commit("src/lib.rs", "pub fn one() {}\n", "feat: one");
    history.write_commit("src/lib.rs", "pub fn two() {}\n", "feat: two");
    // Uncommitted changes that checking out the previous commit would overwrite.
    fs_err::write(
        history.repo.directory().join("src/lib.rs"),
        "pub fn dirty() {}\n",
    )
    .unwrap();
    let error = format!(
        "{:#}",
        history
            .try_diff_with(None, None, |request| request)
            .unwrap_err()
    );
    assert!(
        error.contains("The allow-dirty option can't be used in this case"),
        "{error}"
    );
}

#[test]
fn a_tip_matching_the_release_releases_nothing_although_its_branches_differ() {
    let history = History::new();
    history.two_merged_siblings();
    // The release already contains both siblings, so nothing is left to release
    // even though neither sibling matches the release on its own: only the merge
    // commit does.
    for file in ["src/one.rs", "src/two.rs"] {
        fs_err::write(history.registry.directory().join(file), "").unwrap();
    }
    history
        .registry
        .add_all_and_commit("published release")
        .unwrap();
    assert!(history.diff(None).commits.is_empty());
}
