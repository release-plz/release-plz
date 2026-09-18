use std::collections::HashSet;

use cargo_metadata::{camino::Utf8PathBuf, semver::Version};
use git_cmd::Repo;
use release_plz_core::fs_utils::Utf8TempDir;

use crate::helpers::{
    TEST_REGISTRY,
    package::{PackageType, TestPackage},
    test_context::TestContext,
};

// Keep commit IDs visible so the assertions detect every extra or missing commit,
// including synthetic dependency updates, without depending on changelog grouping.
const CONFIG: &str = r###"
[workspace]
semver_check = false
changelog_path = "CHANGELOG.md"
[changelog]
body = "## {{ version }}\n{% for commit in commits %}- {{ commit.id }} {{ commit.raw_message }}\n{% endfor %}"
"###;

async fn new_context() -> TestContext {
    let context = TestContext::new().await;
    fs_err::write(context.repo_dir().join("src/lib.rs"), "").unwrap();
    context.write_release_plz_toml(CONFIG);
    context
}

/// Local manifests omit the published SHA, exercising equality as the only boundary.
async fn released_sources(context: &TestContext) -> (Utf8TempDir, Utf8PathBuf) {
    context.run_cargo_publish(&context.gitea.repo);
    let temp = Utf8TempDir::new().unwrap();
    let package = context.download_package(temp.path()).await.remove(0);
    (temp, package.manifest_path)
}

fn update(context: &TestContext, args: &[&str]) -> Vec<String> {
    let tip = context.repo.current_commit_hash().unwrap();
    context.update_command().args(args).assert().success();
    assert_eq!(context.repo.current_commit_hash().unwrap(), tip);
    if !context.repo_dir().join("CHANGELOG.md").exists() {
        context.repo.is_clean().unwrap();
        return Vec::new();
    }
    context
        .read_changelog()
        .lines()
        .filter_map(|line| line.strip_prefix("- ").map(str::to_owned))
        .collect()
}

fn write_commit(repo: &Repo, path: &str, contents: &str, message: &str) -> String {
    fs_err::write(repo.directory().join(path), contents).unwrap();
    repo.add_all_and_commit(message).unwrap();
    repo.current_commit_hash().unwrap()
}

fn write_commit_at(repo: &Repo, path: &str, contents: &str, message: &str, day: u8) -> String {
    fs_err::write(repo.directory().join(path), contents).unwrap();
    repo.git(&["add", "."]).unwrap();
    repo.git_at(
        &["commit", "-m", message],
        &format!("2000-01-{day:02}T00:00:00 +0000"),
    )
    .unwrap();
    repo.current_commit_hash().unwrap()
}

fn walk_order(repo: &Repo) -> String {
    repo.git(&["rev-list", "--date-order", "HEAD", "--", "."])
        .unwrap()
}

fn commit_ids(entries: &[String]) -> Vec<&str> {
    entries
        .iter()
        .map(|entry| entry.split_once(' ').unwrap().0)
        .collect()
}

fn assert_commits(entries: &[String], expected: &[&str]) {
    let mut actual = commit_ids(entries);
    actual.sort_unstable();
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected, "{entries:?}");
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn sibling_commits_are_collected_with_tag_published_sha_or_equality_boundary() {
    for boundary in ["tag", "sha", "equality"] {
        let context = new_context().await;
        let (_download, manifest) = released_sources(&context).await;
        let repo = &context.repo;
        let (baseline, one, two) = two_merged_siblings(repo);
        if boundary == "tag" {
            repo.git(&["tag", "v0.1.0", &baseline]).unwrap();
        }
        let args = if boundary == "sha" {
            vec![]
        } else {
            vec!["--registry-manifest-path", manifest.as_str()]
        };
        assert_commits(&update(&context, &args), &[&one, &two]);
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn the_published_commit_bounds_the_walk_without_an_equal_snapshot() {
    let context = new_context().await;
    let repo = &context.repo;
    let published = write_commit(repo, "src/released.rs", "", "feat: released");
    fs_err::write(
        repo.directory().join("src/released.rs"),
        "// published from a dirty tree\n",
    )
    .unwrap();
    assert_cmd::Command::new("cargo")
        .current_dir(repo.directory())
        .env("CARGO_TARGET_DIR", context.cargo_target_dir())
        .env(
            cargo_utils::cargo_registries_token_env_var_name(TEST_REGISTRY).unwrap(),
            format!("Bearer {}", context.gitea.token),
        )
        .args(["publish", "--allow-dirty", "--registry", TEST_REGISTRY])
        .assert()
        .success();
    repo.git(&["restore", "src/released.rs"]).unwrap();
    let unreleased = write_commit(repo, "src/unreleased.rs", "", "feat: unreleased");

    // Reading the same published sources through a local manifest drops the SHA.
    // Prove that equality alone cannot exclude the published commit in this fixture.
    let download = Utf8TempDir::new().unwrap();
    let packages = context.download_package(download.path()).await;
    assert_eq!(packages.len(), 1);
    let entries = update(
        &context,
        &[
            "--registry-manifest-path",
            packages[0].manifest_path.as_str(),
        ],
    );
    repo.git(&["reset", "--hard"]).unwrap();
    fs_err::remove_file(repo.directory().join("CHANGELOG.md")).unwrap();
    assert!(commit_ids(&entries).contains(&published.as_str()));
    assert_commits(&update(&context, &[]), &[&unreleased]);
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn a_published_commit_missing_from_the_repository_is_ignored() {
    let context = new_context().await;
    let repo = &context.repo;
    let baseline = repo.current_commit_hash().unwrap();
    repo.git(&["commit", "--allow-empty", "-m", "chore: publish"])
        .unwrap();
    let published = repo.current_commit_hash().unwrap();
    context.run_cargo_publish(&context.gitea.repo);
    repo.git(&["reset", "--hard", &baseline]).unwrap();
    repo.git(&["reflog", "expire", "--expire=now", "--all"])
        .unwrap();
    repo.git(&["gc", "--prune=now"]).unwrap();
    assert!(repo.git(&["cat-file", "-e", &published]).is_err());
    let unreleased = write_commit(repo, "src/unreleased.rs", "", "feat: unreleased");
    assert_commits(&update(&context, &[]), &[&unreleased]);
}

/// Two sibling branches off the current commit, each merged back with a
/// `--no-ff` merge. Returns the baseline and the two sibling commits.
fn two_merged_siblings(repo: &Repo) -> (String, String, String) {
    let baseline = repo.current_commit_hash().unwrap();
    repo.git(&["checkout", "-b", "one"]).unwrap();
    let one = write_commit(repo, "src/one.rs", "", "fix: sibling one");
    repo.git(&["checkout", "-b", "two", &baseline]).unwrap();
    let two = write_commit(repo, "src/two.rs", "", "fix: sibling two");
    repo.checkout_head().unwrap();
    for branch in ["one", "two"] {
        repo.git(&["merge", "--no-ff", "-m", "merge sibling", branch])
            .unwrap();
    }
    (baseline, one, two)
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn late_merge_keeps_mainline_changes_after_the_release() {
    let history = new_context().await;
    let repo = &history.repo;
    repo.git(&["checkout", "-b", "old-branch"]).unwrap();
    let branch = write_commit(repo, "src/branch.rs", "", "fix: old branch");
    repo.checkout_head().unwrap();
    write_commit(repo, "src/released.rs", "", "feat: already released");
    history.run_cargo_publish(&history.gitea.repo);
    repo.tag_lightweight("v0.1.0").unwrap();
    let mainline = write_commit(repo, "src/mainline.rs", "", "fix: mainline");
    repo.git(&["merge", "--no-ff", "-m", "merge old branch", "old-branch"])
        .unwrap();
    assert_commits(&update(&history, &[]), &[&branch, &mainline]);
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn final_revert_does_not_release_reverted_changes() {
    for tagged in [true, false] {
        let history = new_context().await;
        let (_download, manifest) = released_sources(&history).await;
        let repo = &history.repo;
        if tagged {
            repo.tag_lightweight("v0.1.0").unwrap();
        }
        write_commit(
            repo,
            "src/lib.rs",
            "pub fn temporary() {}\n",
            "feat: temporary",
        );
        write_commit(repo, "src/lib.rs", "", "revert: temporary");
        assert!(update(&history, &["--registry-manifest-path", manifest.as_str()]).is_empty());
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn equal_snapshot_excludes_its_ancestors_but_keeps_sibling_changes() {
    let history = new_context().await;
    let (_download, manifest) = released_sources(&history).await;
    let repo = &history.repo;
    repo.git(&["checkout", "-b", "branch"]).unwrap();
    write_commit(
        repo,
        "src/lib.rs",
        "pub fn temporary() {}\n",
        "feat: temporary",
    );
    let equal = write_commit(repo, "src/lib.rs", "", "revert: temporary");
    let branch = write_commit(repo, "src/branch.rs", "", "fix: branch");
    repo.checkout_head().unwrap();
    // Date the sibling before the branch, so the walk reaches the equal snapshot
    // first: that's the order in which stopping there would lose the sibling.
    let sibling = write_commit_at(repo, "src/sibling.rs", "", "fix: sibling", 2);
    repo.git(&["merge", "--no-ff", "-m", "merge branch", "branch"])
        .unwrap();
    let order = walk_order(repo);
    assert!(order.find(&equal).unwrap() < order.find(&sibling).unwrap());
    assert_commits(
        &update(&history, &["--registry-manifest-path", manifest.as_str()]),
        &[&branch, &sibling],
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn every_lineage_stops_at_its_own_equal_snapshot() {
    let history = new_context().await;
    let (_download, manifest) = released_sources(&history).await;
    let repo = &history.repo;
    let baseline = repo.current_commit_hash().unwrap();
    let branches =
        [("one", 2, 4, 6), ("two", 1, 3, 5)].map(|(branch, reverted_day, equal_day, fix_day)| {
            repo.git(&["checkout", "-b", branch, &baseline]).unwrap();
            let reverted = write_commit_at(
                repo,
                "src/lib.rs",
                &format!("pub fn {branch}() {{}}\n"),
                &format!("feat: {branch}"),
                reverted_day,
            );
            let equal = write_commit_at(
                repo,
                "src/lib.rs",
                "",
                &format!("revert: {branch}"),
                equal_day,
            );
            let fix = write_commit_at(
                repo,
                &format!("src/{branch}.rs"),
                "",
                &format!("fix: {branch}"),
                fix_day,
            );
            (reverted, equal, fix)
        });
    repo.checkout_head().unwrap();
    for (branch, date) in [
        ("one", "2000-01-07T00:00:00 +0000"),
        ("two", "2000-01-08T00:00:00 +0000"),
    ] {
        repo.git_at(&["merge", "--no-ff", "-m", "merge fix", branch], date)
            .unwrap();
    }
    let [
        (reverted_one, equal_one, one),
        (reverted_two, equal_two, two),
    ] = branches;
    // Edit the line both branches reverted: undoing either reverted change at
    // HEAD conflicts, so only its lineage can prove it was discarded.
    let unreleased = write_commit_at(
        repo,
        "src/lib.rs",
        "pub fn unreleased() {}\n",
        "feat: unreleased",
        9,
    );
    // The dates make the walk find the first equal snapshot before the second, and
    // the second before the change reverted by the first: the second snapshot must
    // neither forget the first one nor keep the lineages it stopped.
    let order = walk_order(repo);
    assert!(order.find(&equal_one).unwrap() < order.find(&equal_two).unwrap());
    assert!(order.find(&equal_two).unwrap() < order.find(&reverted_one).unwrap());
    assert!(order.find(&equal_two).unwrap() < order.find(&reverted_two).unwrap());
    let entries = update(&history, &["--registry-manifest-path", manifest.as_str()]);
    assert_commits(&entries, &[&one, &two, &unreleased]);
}

/// A merge with `-s ours` hides real ancestors from simplified history. A second
/// merge exposes them again; equality pruning must exclude them in either visit order.
#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn discarded_ancestors_are_pruned_before_or_after_the_equal_snapshot() {
    for (discarded_day, visited_first) in [(1, false), (6, true)] {
        let history = new_context().await;
        let (_download, manifest) = released_sources(&history).await;
        let repo = &history.repo;
        repo.git(&["checkout", "-b", "feature"]).unwrap();
        let discarded = write_commit_at(
            repo,
            "src/feature.rs",
            "",
            "feat: discarded by the merge",
            discarded_day,
        );
        repo.checkout_head().unwrap();
        write_commit_at(
            repo,
            "src/lib.rs",
            "pub fn temporary() {}\n",
            "feat: temporary",
            2,
        );
        // "Keep mine": the merge commit has the same tree as its first parent, which is
        // what makes git prune the feature branch from walks rooted after it.
        repo.git_at(
            &["merge", "-s", "ours", "-m", "merge feature", "feature"],
            "2000-01-03T00:00:00 +0000",
        )
        .unwrap();
        // Back to the released tree, so this commit is the equal snapshot.
        let equal = write_commit_at(repo, "src/lib.rs", "", "revert: temporary", 4);
        // A second merge of the same branch makes the discarded commit reachable again
        // from HEAD, this time through a merge that git doesn't simplify away.
        repo.git(&["checkout", "feature"]).unwrap();
        let unreleased = write_commit_at(repo, "src/feature2.rs", "", "feat: unreleased", 7);
        repo.checkout_head().unwrap();
        repo.git_at(
            &["merge", "--no-ff", "-m", "merge feature again", "feature"],
            "2000-01-08T00:00:00 +0000",
        )
        .unwrap();
        assert!(repo.is_ancestor(&discarded, &equal));
        let order = walk_order(repo);
        assert_eq!(
            order.find(&discarded).unwrap() < order.find(&equal).unwrap(),
            visited_first,
            "{order}"
        );
        assert_commits(
            &update(&history, &["--registry-manifest-path", manifest.as_str()]),
            &[&unreleased],
        );
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn workspace_dependency_updates_are_detected_without_package_commits() {
    for update_lockfile in [false, true] {
        let context = TestContext::new_workspace_with_packages(&[
            TestPackage::new("app"),
            TestPackage::new("dep").with_type(PackageType::Lib),
        ])
        .await;
        let repo = &context.repo;
        context.write_release_plz_toml(CONFIG);
        context.run_cargo_publish("dep");
        let root_manifest = repo.directory().join("Cargo.toml");
        let root = fs_err::read_to_string(&root_manifest).unwrap();
        fs_err::write(&root_manifest, format!("{root}\n[workspace.dependencies]\ndep = {{ version = \"0.1\", registry = \"{TEST_REGISTRY}\" }}\n")).unwrap();
        let app_manifest = context.package_path("app").join("Cargo.toml");
        let app = fs_err::read_to_string(&app_manifest).unwrap();
        fs_err::write(
            app_manifest,
            app.replace("[dependencies]", "[dependencies]\ndep.workspace = true"),
        )
        .unwrap();
        context.run_cargo_check();
        context.push_all_changes("chore: workspace dependency");
        context.run_cargo_publish("app");
        repo.tag_lightweight("app-v0.1.0").unwrap();
        assert!(update(&context, &["-p", "app"]).is_empty());
        let expected = if update_lockfile {
            context.set_package_version("dep", &Version::new(0, 1, 1));
            context.push_all_changes("chore: dep version");
            context.run_cargo_publish("dep");
            assert_cmd::Command::new("cargo")
                .current_dir(repo.directory())
                .args(["update", "-p", "dep@0.1.0", "--precise", "0.1.1"])
                .assert()
                .success();
            "Cargo.lock"
        } else {
            let manifest = fs_err::read_to_string(&root_manifest).unwrap();
            fs_err::write(root_manifest, manifest.replace("\"0.1\"", "\">=0.1.0\"")).unwrap();
            "Cargo.toml"
        };
        context.run_cargo_check();
        context.push_all_changes("chore: workspace dependencies");
        assert!(
            repo.git(&["rev-list", "app-v0.1.0..HEAD", "--", "crates/app"])
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            update(&context, &["-p", "app"]),
            [format!("0000000 chore: update {expected} dependencies")]
        );
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn first_release_respects_the_commit_limit() {
    for limit in [1, 2, 0] {
        let history = new_context().await;
        let repo = &history.repo;
        write_commit(repo, "src/one.rs", "", "fix: one");
        write_commit(repo, "src/two.rs", "", "fix: two");
        // Includes the setup commits when zero disables the limit, newest first.
        let log = repo.git(&["rev-list", "HEAD"]).unwrap();
        let expected: Vec<_> = log
            .lines()
            .take(if limit == 0 { usize::MAX } else { limit })
            .collect();
        assert_eq!(
            commit_ids(&update(
                &history,
                &["--max-analyze-commits", &limit.to_string()]
            )),
            expected
        );
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn a_tag_bounds_the_history_of_a_package_that_is_not_published() {
    let history = new_context().await;
    history.write_release_plz_toml(&CONFIG.replace(
        "semver_check = false",
        "semver_check = false\npublish = false",
    ));
    let repo = &history.repo;
    write_commit(repo, "src/released.rs", "", "feat: released by the tag");
    history.repo.tag_lightweight("v0.1.0").unwrap();
    let unreleased = write_commit(repo, "src/unreleased.rs", "", "feat: after the tag");
    assert_commits(&update(&history, &[]), &[&unreleased]);
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn a_blocking_dirty_working_tree_hints_at_the_allow_dirty_option() {
    use release_plz_core::{Project, update_request::UpdateRequest, updater::Updater};

    let history = new_context().await;
    let repo = &history.repo;
    write_commit(repo, "src/lib.rs", "pub fn one() {}\n", "feat: one");
    write_commit(repo, "src/lib.rs", "pub fn two() {}\n", "feat: two");
    fs_err::write(repo.directory().join("src/lib.rs"), "pub fn dirty() {}\n").unwrap();
    // The CLI stashes dirty files first. Exercise the public updater directly to
    // retain coverage of its diagnostic when a history checkout would overwrite them.
    let manifest = repo.directory().join("Cargo.toml");
    let metadata = cargo_utils::get_manifest_metadata(&manifest).unwrap();
    let request = UpdateRequest::new(metadata.clone()).unwrap();
    let project = Project::new(&manifest, None, &HashSet::new(), &metadata, &request).unwrap();
    #[expect(
        clippy::default_trait_access,
        reason = "the registry collection type is not exported"
    )]
    let error = Updater {
        project: &project,
        req: &request,
    }
    .packages_to_update(&Default::default(), repo, &manifest)
    .await
    .unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.contains("The allow-dirty option can't be used in this case"),
        "{error}"
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "docker-tests"), ignore)]
async fn a_tip_matching_the_release_releases_nothing_although_its_branches_differ() {
    let history = new_context().await;
    let repo = &history.repo;
    two_merged_siblings(repo);
    // The release already contains both siblings, so nothing is left to release
    // even though neither sibling matches the release on its own: only the merge
    // commit does.
    let (_download, manifest) = released_sources(&history).await;
    assert!(update(&history, &["--registry-manifest-path", manifest.as_str()]).is_empty());
}
