use super::*;
use crate::{
    command::update::UpdateConfig,
    test_utils::{generate_lockfile, write_package},
};

const PACKAGE: &str = "history-test";

struct History {
    repo: Repo,
    registry: Repo,
    _dir: fs_utils::Utf8TempDir,
}

impl History {
    fn new() -> Self {
        Self::with_packages(|root| write_package(root, PACKAGE, "0.1.0", ""))
    }

    fn with_packages(write_packages: impl Fn(&Utf8Path)) -> Self {
        let dir = fs_utils::Utf8TempDir::new().unwrap();
        // Resolve symlinks (such as macOS's /var) so metadata and project paths agree.
        let root = fs_utils::canonicalize_utf8(dir.path()).unwrap();
        let [repo, registry] = ["local", "registry"].map(|name| {
            let path = root.join(name);
            fs_err::create_dir(&path).unwrap();
            let repo = Repo::init(path);
            // Keep checked-out files byte-identical to the LF-only registry fixtures.
            repo.git(&["config", "core.autocrlf", "false"]).unwrap();
            write_packages(repo.directory());
            fs_err::write(repo.directory().join(".gitignore"), "/target\n").unwrap();
            generate_lockfile(repo.directory());
            repo.add_all_and_commit("chore: published baseline")
                .unwrap();
            repo
        });
        Self {
            repo,
            registry,
            _dir: dir,
        }
    }

    fn write_commit(&self, path: &str, contents: &str, message: &str) -> String {
        fs_err::write(self.repo.directory().join(path), contents).unwrap();
        self.repo.add_all_and_commit(message).unwrap();
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
            .git(&["merge", "-s", "ours", "-m", "merge equal", "equal"])
            .unwrap();
        self.repo.git(&["checkout", "equal"]).unwrap();
        let sibling = self.write_commit(sibling_path, "", "fix: sibling");
        self.repo.checkout_head().unwrap();
        self.repo
            .git(&["merge", "--no-ff", "-m", "merge sibling", "equal"])
            .unwrap();
        sibling
    }

    /// Set both dates to control the commit's position in the date-ordered walk.
    fn write_commit_at(&self, path: &str, contents: &str, message: &str, day: u8) -> String {
        fs_err::write(self.repo.directory().join(path), contents).unwrap();
        self.repo.git(&["add", "."]).unwrap();
        self.repo
            .git_at(
                &["commit", "-m", message],
                &format!("2000-01-{day:02}T00:00:00 +0000"),
            )
            .unwrap();
        self.repo.current_commit_hash().unwrap()
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

    /// Every commit in the order `get_diff` visits them, newest date first.
    fn walk_order(&self) -> String {
        self.repo
            .git(&["rev-list", "--date-order", "HEAD", "--", "."])
            .unwrap()
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
            &self.request(),
        )
        .unwrap()
    }

    fn request(&self) -> UpdateRequest {
        let metadata =
            cargo_utils::get_manifest_metadata(&self.repo.directory().join(CARGO_TOML)).unwrap();
        UpdateRequest::new(metadata).unwrap()
    }

    fn diff_with(
        &self,
        published: Option<RegistryPackage>,
        request: &UpdateRequest,
    ) -> anyhow::Result<Diff> {
        let tip = self.repo.current_commit_hash().unwrap();
        let metadata = request.cargo_metadata();
        let package = cargo_utils::workspace_package(metadata, PACKAGE).unwrap();
        let project = Project::new(
            request.local_manifest(),
            None,
            &HashSet::new(),
            metadata,
            request,
        )
        .unwrap();
        let registry_packages = PackagesCollection::default().with_packages(
            published
                .into_iter()
                .map(|p| (p.package.name.to_string(), p))
                .collect(),
        );
        let diff = Updater {
            project: &project,
            req: request,
        }
        .get_diff(package, &registry_packages, &self.repo)?;
        assert_eq!(self.repo.current_commit_hash().unwrap(), tip);
        Ok(diff)
    }
}

fn commit_ids(diff: &Diff) -> Vec<&str> {
    diff.commits
        .iter()
        .map(|commit| commit.id.as_str())
        .collect()
}

fn assert_commits(diff: &Diff, expected: &[&str]) {
    let mut actual = commit_ids(diff);
    actual.sort_unstable();
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected, "{:?}", diff.commits);
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
fn conflict_resolution_can_preserve_a_change_reverted_on_another_branch() {
    let history = api_history();
    let repo = &history.repo;
    let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    repo.git(&["checkout", "-b", "equal"]).unwrap();
    history.write_commit("src/lib.rs", BASE_API, "revert: breaking API");
    repo.checkout_head().unwrap();
    let prepared = history.write_commit(
        "src/lib.rs",
        &BASE_API.replace("api()", "api(_: u8)"),
        "chore: prepare merge",
    );
    assert!(
        repo.git(&["merge", "--no-ff", "--no-commit", "equal"])
            .is_err()
    );
    let resolved = history.write_commit("src/lib.rs", BREAKING_API, "merge resolved");
    repo.git(&["checkout", "equal"]).unwrap();
    let sibling = history.write_commit(
        "src/lib.rs",
        &format!("{BASE_API}pub fn extra() {{}}\n"),
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
    // Both merges resolve `src/lib.rs` to contents that differ from all their parents.
    assert_commits(&diff, &[&breaking, &prepared, &resolved, &sibling, &merge]);
    assert_next_version(&diff, &Version::new(0, 2, 0));
}

#[test]
fn a_release_side_conflict_keeps_a_breaking_change_absent_from_the_release() {
    let history = api_history();
    let released_api = BASE_API.replace("api()", "api(_: u8)");
    fs_err::write(
        history.registry.directory().join("src/lib.rs"),
        &released_api,
    )
    .unwrap();
    history
        .registry
        .add_all_and_commit("published different API")
        .unwrap();
    let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    let sibling = history.merge_ignored_revert("src/lib.rs", Some(&released_api));

    // Undoing `api(bool)` to `api()` conflicts with the released `api(u8)`.
    // HEAD still has `api(bool)`, so its breaking-change marker must survive.
    let diff = history.diff(None);
    assert_commits(&diff, &[&breaking, &sibling]);
    assert_next_version(&diff, &Version::new(0, 2, 0));
}

#[test]
fn an_already_released_breaking_change_is_not_repeated_after_body_edits() {
    let history = api_history();
    let released_api = BREAKING_API.replace(
        "api(_: bool) {}",
        "api(_: bool) { /* published implementation */ }",
    );
    fs_err::write(
        history.registry.directory().join("src/lib.rs"),
        &released_api,
    )
    .unwrap();
    history
        .registry
        .add_all_and_commit("published implementation")
        .unwrap();
    history.write_commit("src/lib.rs", BREAKING_API, "feat!: already released API");
    let sibling = history.merge_ignored_revert("src/lib.rs", Some(&released_api));

    // The release already contains the breaking signature. Its later body edit
    // must not make a line-level revert conflict look like an absent signature.
    let diff = history.diff(None);
    assert_commits(&diff, &[&sibling]);
    assert_next_version(&diff, &Version::new(0, 1, 1));
}

#[test]
fn a_retained_api_deletion_keeps_its_breaking_change_marker() {
    for boundary in ["tag", "published", "missing", "equality"] {
        let history = api_history();
        let baseline = history.repo.current_commit_hash().unwrap();
        let missing = "0".repeat(40);
        let published_at = match boundary {
            "tag" => {
                history.repo.tag_lightweight("v0.1.0").unwrap();
                None
            }
            "published" => Some(baseline.as_str()),
            "missing" => Some(missing.as_str()),
            _ => None,
        };
        let breaking =
            history.write_commit("src/lib.rs", "pub fn stable() {}\n", "feat!: remove API");
        let sibling = history.merge_ignored_revert("src/lib.rs", Some(BASE_API));
        let diff = history.diff(published_at);
        assert_commits(&diff, &[&breaking, &sibling]);
        assert_next_version(&diff, &Version::new(0, 2, 0));
    }
}

#[test]
fn a_retained_change_can_move_to_a_different_file() {
    let history = api_history();
    let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    let sibling = history.merge_ignored_revert("src/lib.rs", Some(BASE_API));
    history
        .repo
        .git(&["mv", "src/lib.rs", "src/api.rs"])
        .unwrap();
    let moved = history.write_commit(
        "src/lib.rs",
        "mod api;\npub use api::*;\n",
        "chore: move API",
    );
    let diff = history.diff(None);
    assert_commits(&diff, &[&breaking, &sibling, &moved]);
    assert_next_version(&diff, &Version::new(0, 2, 0));
}

#[test]
fn sibling_commits_are_collected_with_tag_published_sha_or_equality_boundary() {
    let history = History::new();
    let repo = &history.repo;
    let (baseline, one, two) = history.two_merged_siblings();
    repo.git(&["tag", "v0.1.0", &baseline]).unwrap();
    let expected = [one.as_str(), two.as_str()];
    assert_commits(&history.diff(None), &expected);
    repo.git(&["tag", "-d", "v0.1.0"]).unwrap();
    for published_at in [Some(baseline.as_str()), None] {
        assert_commits(&history.diff(published_at), &expected);
    }
}

/// A release from a dirty tree can differ from every committed snapshot.
#[test]
fn the_published_commit_bounds_the_walk_without_an_equal_snapshot() {
    let history = History::new();
    let published = history.write_commit("src/released.rs", "", "feat: released");
    fs_err::write(
        history.registry.directory().join("src/released.rs"),
        "// published from a modified working tree\n",
    )
    .unwrap();
    history
        .registry
        .add_all_and_commit("published release")
        .unwrap();
    let unreleased = history.write_commit("src/unreleased.rs", "", "feat: unreleased");
    // No local snapshot equals the release, so nothing else bounds the walk.
    assert!(commit_ids(&history.diff(None)).contains(&published.as_str()));
    assert_commits(&history.diff(Some(&published)), &[&unreleased]);
}

/// A history rewrite can remove the published commit; ignore that boundary.
#[test]
fn a_published_commit_missing_from_the_repository_is_ignored() {
    let history = History::new();
    let unreleased = history.write_commit("src/unreleased.rs", "", "feat: unreleased");
    let missing = "0".repeat(40);
    assert_commits(&history.diff(Some(&missing)), &[&unreleased]);
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
    assert_commits(&history.diff(None), &[&branch, &mainline]);
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
    let sibling = history.write_commit_at("src/sibling.rs", "", "fix: sibling", 2);
    repo.git(&["merge", "--no-ff", "-m", "merge branch", "branch"])
        .unwrap();
    let order = history.walk_order();
    assert!(order.find(&equal).unwrap() < order.find(&sibling).unwrap());
    assert_commits(&history.diff(None), &[&branch, &sibling]);
}

/// Each branch reverts to the released tree before contributing a fix.
#[test]
fn every_lineage_stops_at_its_own_equal_snapshot() {
    let history = History::new();
    let repo = &history.repo;
    let baseline = repo.current_commit_hash().unwrap();
    repo.git(&["checkout", "-b", "one"]).unwrap();
    let reverted_one = history.write_commit_at("src/lib.rs", "pub fn one() {}\n", "feat: one", 2);
    let equal_one = history.write_commit_at("src/lib.rs", "", "revert: one", 4);
    let one = history.write_commit_at("src/one.rs", "", "fix: one", 6);
    repo.git(&["checkout", "-b", "two", &baseline]).unwrap();
    let reverted_two = history.write_commit_at("src/lib.rs", "pub fn two() {}\n", "feat: two", 1);
    let equal_two = history.write_commit_at("src/lib.rs", "", "revert: two", 3);
    let two = history.write_commit_at("src/two.rs", "", "fix: two", 5);
    repo.checkout_head().unwrap();
    for (branch, date) in [
        ("one", "2000-01-07T00:00:00 +0000"),
        ("two", "2000-01-08T00:00:00 +0000"),
    ] {
        repo.git_at(&["merge", "--no-ff", "-m", "merge fix", branch], date)
            .unwrap();
    }
    // Edit the line both branches reverted: undoing either reverted change at
    // HEAD conflicts, so only its lineage can prove it was discarded.
    let unreleased = history.write_commit_at(
        "src/lib.rs",
        "pub fn unreleased() {}\n",
        "feat: unreleased",
        9,
    );
    // The dates make the walk find the first equal snapshot before the second, and
    // the second before the change reverted by the first: the second snapshot must
    // neither forget the first one nor keep the lineages it stopped.
    let order = history.walk_order();
    assert!(order.find(&equal_one).unwrap() < order.find(&equal_two).unwrap());
    assert!(order.find(&equal_two).unwrap() < order.find(&reverted_one).unwrap());
    assert!(order.find(&equal_two).unwrap() < order.find(&reverted_two).unwrap());
    assert_commits(&history.diff(None), &[&one, &two, &unreleased]);
}

/// History simplification hides one parent of an "ours" merge. Pruning an equal
/// snapshot must still remove that ancestor, whichever one the walk visits first.
#[test]
fn discarded_ancestors_are_pruned_in_either_visit_order() {
    for (discarded_day, discarded_first) in [(1, false), (6, true)] {
        let history = History::new();
        let repo = &history.repo;
        repo.git(&["checkout", "-b", "feature"]).unwrap();
        let discarded = history.write_commit_at(
            "src/feature.rs",
            "",
            "feat: discarded by the merge",
            discarded_day,
        );
        repo.checkout_head().unwrap();
        history.write_commit_at(
            "src/lib.rs",
            "pub fn temporary() {}\n",
            "feat: temporary",
            2,
        );
        // Keep the mainline tree, discarding the feature branch's changes.
        repo.git_at(
            &["merge", "-s", "ours", "-m", "merge feature", "feature"],
            "2000-01-03T00:00:00 +0000",
        )
        .unwrap();
        let equal = history.write_commit_at("src/lib.rs", "", "revert: temporary", 4);
        // A second merge makes the discarded commit visible in the walk again.
        repo.git(&["checkout", "feature"]).unwrap();
        let unreleased = history.write_commit_at("src/feature2.rs", "", "feat: unreleased", 7);
        repo.checkout_head().unwrap();
        repo.git_at(
            &["merge", "--no-ff", "-m", "merge feature again", "feature"],
            "2000-01-08T00:00:00 +0000",
        )
        .unwrap();

        assert!(repo.is_ancestor(&discarded, &equal));
        let order = history.walk_order();
        assert_eq!(
            order.find(&discarded).unwrap() < order.find(&equal).unwrap(),
            discarded_first,
            "{order}"
        );
        assert_commits(&history.diff(None), &[&unreleased]);
    }
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
    // Zero means no limit. Commits must be collected newest first.
    let expected: [&str; 4] = [&two, &one, &baseline, &readme];
    for (limit, expected) in [(1, &expected[..1]), (2, &expected[..2]), (0, &expected[..])] {
        let request = history.request().with_max_analyze_commits(Some(limit));
        let diff = history.diff_with(None, &request).unwrap();
        assert_eq!(commit_ids(&diff), expected, "commit limit: {limit}");
    }
}

/// Unpublished packages have only the tag to bound their history.
#[test]
fn a_tag_bounds_the_history_of_a_package_that_is_not_published() {
    let history = History::new();
    history.write_commit("src/released.rs", "", "feat: released by the tag");
    history.repo.tag_lightweight("v0.1.0").unwrap();
    let unreleased = history.write_commit("src/unreleased.rs", "", "feat: after the tag");
    let request = history.request().with_default_package_config(UpdateConfig {
        publish: false,
        ..UpdateConfig::default()
    });
    let diff = history.diff_with(None, &request).unwrap();
    assert_commits(&diff, &[&unreleased]);
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
        history.diff_with(None, &history.request()).unwrap_err()
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
