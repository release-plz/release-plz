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

    /// Ignore a revert of the current API change, then import a sibling change
    /// through the reverted branch so that its equal snapshot is also visited.
    fn merge_ignored_api_revert(&self) -> String {
        self.repo.git(&["checkout", "-b", "equal"]).unwrap();
        self.write_commit("src/lib.rs", BASE_API, "revert: breaking API");
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
        let sibling = self.write_commit("src/fix.rs", "", "fix: sibling");
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
fn an_ignored_revert_does_not_hide_a_surviving_breaking_change() {
    for skew in [false, true] {
        for tagged in [false, true] {
            let history = api_history();
            let repo = &history.repo;
            if tagged {
                repo.tag_lightweight("v0.1.0").unwrap();
            }
            let breaking = history.write_commit_at(
                "src/lib.rs",
                BREAKING_API,
                "feat!: breaking API",
                "2000-01-05T00:00:00 +0000",
            );
            repo.git(&["checkout", "-b", "equal"]).unwrap();
            history.write_commit_at(
                "src/lib.rs",
                BASE_API,
                "revert: breaking API",
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
                format!("{BREAKING_API}pub fn extra() {{}}\n")
            );
            let diff = history.diff(None);
            assert_eq!(
                commit_ids(&diff),
                HashSet::from([breaking.as_str(), sibling.as_str(), merge.as_str()]),
                "skew={skew}, tagged={tagged}: {:?}",
                diff.commits
            );
            assert_next_version(&diff, &Version::new(0, 2, 0));
        }
    }
}

#[test]
fn a_discarded_change_stays_excluded_when_a_sibling_changes_the_same_file() {
    for discarded_date in ["2000-01-02T00:00:00 +0000", "2000-01-08T00:00:00 +0000"] {
        let history = api_history();
        let repo = &history.repo;
        repo.git(&["checkout", "-b", "feature"]).unwrap();
        history.write_commit_at(
            "src/lib.rs",
            BREAKING_API,
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
            &format!("{BREAKING_API}pub fn extra() {{}}\n"),
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
            "{:?}",
            diff.commits
        );
        assert_next_version(&diff, &Version::new(0, 1, 1));
    }
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
    for ignored in ["ignored.txt", "Cargo.lock"] {
        let history = History::with_packages(|root| {
            write_package(root, PACKAGE, "0.1.0", "exclude = [\"ignored.txt\"]\n");
            fs_err::write(root.join("src/lib.rs"), BASE_API).unwrap();
            fs_err::write(root.join("ignored.txt"), "original\n").unwrap();
        });
        let path = history.repo.directory().join(ignored);
        let old = fs_err::read_to_string(&path).unwrap();
        fs_err::write(path, format!("{old}# changed\n")).unwrap();
        let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
        let sibling = history.merge_ignored_api_revert();
        let diff = history.diff(None);
        assert_eq!(
            commit_ids(&diff),
            HashSet::from([breaking.as_str(), sibling.as_str()]),
            "ignored={ignored}"
        );
        assert_next_version(&diff, &Version::new(0, 2, 0));
    }
}

#[test]
fn a_retained_api_deletion_keeps_its_breaking_change_marker() {
    let history = api_history();
    let breaking = history.write_commit("src/lib.rs", "pub fn stable() {}\n", "feat!: remove API");
    let sibling = history.merge_ignored_api_revert();
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
    history.merge_ignored_api_revert();
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
