//! Exercise history selection through the CLI and packages published to Gitea.

use cargo_metadata::camino::Utf8Path;
use cargo_utils::CARGO_TOML;
use git_cmd::Repo;

use crate::helpers::{
    locked_metadata,
    package::{PackageType, TestPackage},
    test_context::TestContext,
};

const PACKAGE: &str = "pkg";
const HISTORY_CONFIG: &str = r#"
[workspace]
semver_check = false

[changelog]
header = ""
body = """
## {{ version }}
{% for commit in commits %}{{ commit.id }} {{ commit.raw_message }}
{% endfor %}
"""
"#;

async fn unpublished_history(contents: &str) -> TestContext {
    let context = TestContext::new().await;
    context
        .repo
        .git(&["config", "core.autocrlf", "false"])
        .unwrap();
    fs_err::write(context.repo.directory().join("src/lib.rs"), contents).unwrap();
    context.write_release_plz_toml(HISTORY_CONFIG);
    context
}

async fn api_history(released: &str) -> TestContext {
    let context = unpublished_history(if released.is_empty() { "" } else { BASE_API }).await;
    context.publish_snapshot(&[("src/lib.rs", released)]);
    context
}

async fn member_history(write_extra: impl FnOnce(&Utf8Path)) -> TestContext {
    let context = TestContext::new_workspace_with_packages(&[
        TestPackage::new(PACKAGE).with_type(PackageType::Lib)
    ])
    .await;
    context
        .repo
        .git(&["config", "core.autocrlf", "false"])
        .unwrap();
    fs_err::write(
        context.repo.directory().join("crates/pkg/src/lib.rs"),
        BASE_API,
    )
    .unwrap();
    write_extra(context.repo.directory());
    // Cargo lists a generated lockfile; keep its path resolvable in historical snapshots.
    fs_err::copy(
        context.repo.directory().join("Cargo.lock"),
        context.package_path(PACKAGE).join("Cargo.lock"),
    )
    .unwrap();
    context.write_release_plz_toml(HISTORY_CONFIG);
    context
}

impl TestContext {
    fn publish_with_boundary(&self, boundary: &str) {
        match boundary {
            "published" => self.run_cargo_publish(self.history_package().name.as_str()),
            "tag" | "equality" | "missing" => {
                let published = self.publish_snapshot(&[]);
                if boundary == "tag" {
                    self.repo.tag_lightweight("v0.1.0").unwrap();
                } else if boundary == "missing" {
                    self.repo
                        .git(&["reflog", "expire", "--expire=now", "--all"])
                        .unwrap();
                    self.repo.git(&["gc", "--prune=now"]).unwrap();
                    assert!(self.repo.git(&["cat-file", "-e", &published]).is_err());
                }
            }
            other => panic!("unknown boundary {other}"),
        }
    }

    fn history_package(&self) -> cargo_metadata::Package {
        locked_metadata(self.repo.directory())
            .workspace_packages()
            .into_iter()
            .find(|p| p.name == PACKAGE || p.name.as_str() == self.gitea.repo)
            .unwrap()
            .clone()
    }

    /// Publish an independent snapshot: its commit cannot bound any local lineage,
    /// so the update has to discover equal package contents during the walk.
    fn publish_snapshot(&self, files: &[(&str, &str)]) -> String {
        self.repo
            .git(&["checkout", "--orphan", "published"])
            .unwrap();
        for (path, contents) in files {
            fs_err::write(self.repo.directory().join(path), contents).unwrap();
        }
        self.repo
            .add_all_and_commit("chore: published snapshot")
            .unwrap();
        let published = self.repo.current_commit_hash().unwrap();
        self.run_cargo_publish(self.history_package().name.as_str());
        self.repo.checkout_head().unwrap();
        self.repo.git(&["branch", "-D", "published"]).unwrap();
        published
    }

    fn assert_release(&self, commits: &[&str], version: &str) {
        assert_release(&self.update_history(), commits, version, "");
    }

    fn update_history(&self) -> String {
        self.check_update(self.update_command())
    }

    fn check_update(&self, mut command: assert_cmd::Command) -> String {
        let package = self.history_package();
        let head = self.repo.current_commit_hash().unwrap();
        command
            .args([
                "--package",
                package.name.as_str(),
                "--repo-url",
                &self.gitea.repo_clone_url(),
            ])
            .assert()
            .success();
        assert_eq!(self.repo.current_commit_hash().unwrap(), head);
        let changelog = package.manifest_path.with_file_name("CHANGELOG.md");
        let contents = if changelog.exists() {
            let contents = fs_err::read_to_string(&changelog).unwrap();
            let version = self.history_package().version;
            assert!(
                contents.trim().starts_with(&format!("## {version}\n")),
                "{contents}"
            );
            fs_err::remove_file(changelog).unwrap();
            contents
        } else {
            String::new()
        };
        // Each assertion can be followed by another edit of the same history.
        self.repo.git(&["reset", "--hard", "HEAD"]).unwrap();
        contents.trim().to_string()
    }

    fn write_commit(&self, path: &str, contents: &str, message: &str) -> String {
        fs_err::write(self.repo.directory().join(path), contents).unwrap();
        self.repo.add_all_and_commit(message).unwrap();
        self.repo.current_commit_hash().unwrap()
    }

    /// Ignore a revert of the current change, then import a sibling change
    /// through the reverted branch so that its equal snapshot is also visited.
    fn merge_ignored_revert(&self, path: &str, contents: &str) -> String {
        self.merge_ignored_change("src/fix.rs", |root| {
            fs_err::write(root.join(path), contents).unwrap();
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

    /// Continue with a clone that fetches each blob only when Git first reads
    /// it, so the files of commits the walk never checks out stay missing
    /// unless the replay fetches them.
    fn partial_clone(self) -> Self {
        // Serving a filtered clone, and the lazy fetches that follow, needs
        // the source's permission.
        self.repo
            .git(&["config", "uploadpack.allowFilter", "true"])
            .unwrap();
        self.clone_with("--filter=blob:none", "partial")
    }

    /// Clone the repository with `option` into the sibling directory `name`.
    fn clone_with(mut self, option: &str, name: &str) -> Self {
        let root = self.repo.directory().parent().unwrap();
        // Local clones copy every object: force the transport that honors
        // depth and filters.
        git_cmd::git_in_dir(
            root,
            &[
                "clone",
                "--no-local",
                option,
                "--config",
                "core.autocrlf=false",
                self.repo.directory().as_str(),
                name,
            ],
        )
        .unwrap();
        self.repo = Repo::new(root.join(name)).unwrap();
        self
    }

    /// The objects reachable from HEAD that this clone does not have.
    fn missing_objects(&self) -> Vec<String> {
        // `--missing` also stops Git from fetching them.
        self.repo
            .git(&["rev-list", "--objects", "--missing=print", "HEAD"])
            .unwrap()
            .lines()
            .filter_map(|line| line.strip_prefix('?'))
            .map(str::to_owned)
            .collect()
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
    /// `--no-ff` merge. Returns the two sibling commits.
    fn two_merged_siblings(&self) -> (String, String) {
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
        (one, two)
    }

    /// Break the API and branch `late` off the breaking
    /// change; then evolve `src/lib.rs` to `released` on the mainline, commit
    /// `after_contents` at `after_path` and merge `late` back. Returns the late
    /// and the after commits.
    fn merge_late_branch(
        &self,
        released: &str,
        after_path: &str,
        after_contents: &str,
    ) -> (String, String) {
        let repo = &self.repo;
        self.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
        repo.git(&["checkout", "-b", "late"]).unwrap();
        let late = self.write_commit("src/late.rs", "", "fix: late branch");
        repo.checkout_head().unwrap();
        self.write_commit("src/lib.rs", released, "chore: evolve API");
        let after = self.write_commit(after_path, after_contents, "fix: after the release");
        repo.git(&["merge", "--no-ff", "-m", "merge late branch", "late"])
            .unwrap();
        (late, after)
    }

    /// Rename the API file to `src/api.rs` and point the manifest at it.
    /// Returns the rename commit.
    fn rename_api_file(&self) -> String {
        self.repo.git(&["mv", "src/lib.rs", "src/api.rs"]).unwrap();
        let manifest = fs_err::read_to_string(self.repo.directory().join(CARGO_TOML)).unwrap();
        self.write_commit(
            CARGO_TOML,
            &format!("{manifest}\n[lib]\npath = \"src/api.rs\"\n"),
            "chore: rename the API file",
        )
    }

    /// Every commit in the order `get_diff` visits them, newest date first.
    fn walk_order(&self) -> String {
        self.repo
            .git(&["rev-list", "--date-order", "HEAD", "--", "."])
            .unwrap()
    }
}

fn commit_ids(changelog: &str) -> Vec<&str> {
    changelog
        .lines()
        .filter(|line| !line.starts_with("## "))
        .filter_map(|line| line.split_once(' ').map(|(id, _)| id))
        .collect()
}

fn assert_commits(changelog: &str, expected: &[&str]) {
    assert_commits_with_context(changelog, expected, "");
}

fn assert_commits_with_context(changelog: &str, expected: &[&str], context: &str) {
    let mut actual = commit_ids(changelog);
    actual.sort_unstable();
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected, "{context}\n{changelog}");
}

fn assert_release(changelog: &str, commits: &[&str], version: &str, context: &str) {
    assert_commits_with_context(changelog, commits, context);
    assert!(
        changelog.starts_with(&format!("## {version}\n")),
        "{context}\n{changelog}"
    );
}

const BASE_API: &str = "pub fn api() {}\n\n\n\n\n\npub fn stable() {}\n";
const BREAKING_API: &str = "pub fn api(_: bool) {}\n\n\n\n\n\npub fn stable() {}\n";
/// A release whose `api` signature differs from both the base and the breaking one.
const RELEASED_API: &str = "pub fn api(_: u8) {}\n\n\n\n\n\npub fn stable() {}\n";

/// [`BASE_API`] with an implementation change that leaves the API untouched.
const IMPLEMENTED_API: &str =
    "pub fn api() { /* implementation */ }\n\n\n\n\n\npub fn stable() {}\n";
/// [`IMPLEMENTED_API`] with the signature of [`BREAKING_API`].
const IMPLEMENTED_BREAKING_API: &str =
    "pub fn api(_: bool) { /* implementation */ }\n\n\n\n\n\npub fn stable() {}\n";

#[cfg(unix)]
#[tokio::test]
async fn executable_bit_changes_do_not_hide_a_retained_package_change() {
    let history = api_history(BASE_API).await;
    assert_retained_changes_with_executable_bit(&history, BREAKING_API, &[]);
}

#[cfg(unix)]
#[tokio::test]
async fn executable_bit_changes_do_not_hide_retained_sequential_package_changes() {
    let history = api_history(BASE_API).await;
    let implementation =
        history.write_commit("src/lib.rs", IMPLEMENTED_API, "chore: implementation");
    assert_retained_changes_with_executable_bit(
        &history,
        IMPLEMENTED_BREAKING_API,
        &[&implementation],
    );
}

#[cfg(unix)]
fn assert_retained_changes_with_executable_bit(
    history: &TestContext,
    breaking_api: &str,
    prior_changes: &[&str],
) {
    use std::os::unix::fs::PermissionsExt;

    // Record the executable bit.
    history
        .repo
        .git(&["config", "core.filemode", "true"])
        .unwrap();
    // Combine the mode and API changes in one commit so replay has to distinguish them.
    fs_err::set_permissions(
        history.repo.directory().join("src/lib.rs"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let breaking = history.write_commit(
        "src/lib.rs",
        breaking_api,
        "feat!: breaking API and set executable bit",
    );
    // Discard the merged branch's revert: HEAD still contains all content changes.
    let sibling = history.merge_ignored_revert("src/lib.rs", BASE_API);
    let mut retained = prior_changes.to_vec();
    retained.extend([breaking.as_str(), sibling.as_str()]);
    history.assert_release(&retained, "0.2.0");

    // Restore the released contents while leaving the executable bit set.
    // All content changes should disappear, leaving only a patch bump.
    let restore = history.write_commit("src/lib.rs", BASE_API, "fix: restore API");
    history.assert_release(&[&restore, &sibling], "0.1.1");
}

#[tokio::test]
async fn nested_cargo_vcs_info_changes_keep_their_breaking_change_marker() {
    // Cargo generates this file at the package root, but under src/ it is
    // an ordinary packaged file whose contents affect the release.
    let path = "src/.cargo_vcs_info.json";
    let history = unpublished_history("").await;
    history.write_commit(path, "{}\n", "chore: add fixture");
    history.publish_snapshot(&[]);
    let breaking = history.write_commit(path, "{\"breaking\":true}\n", "feat!: fixture format");
    let sibling = history.merge_ignored_revert(path, "{}\n");
    history.assert_release(&[&breaking, &sibling], "0.2.0");
}

/// Files whose contents package equality ignores, while their presence below
/// the package root still changes the packaged file list.
const NESTED_METADATA_FILES: [&str; 2] = ["src/Cargo.lock", "src/Cargo.toml.orig"];

#[tokio::test]
async fn nested_metadata_file_additions_keep_their_breaking_change_marker() {
    for path in NESTED_METADATA_FILES {
        let history = api_history(BASE_API).await;
        let breaking = history.write_commit(path, "fixture\n", "feat!: add fixture");
        let sibling = history.merge_ignored_change("src/fix.rs", |root| {
            fs_err::remove_file(root.join(path)).unwrap();
        });
        history.assert_release(&[&breaking, &sibling], "0.2.0");
    }
}

#[tokio::test]
async fn nested_metadata_file_deletions_keep_their_breaking_change_marker() {
    for path in NESTED_METADATA_FILES {
        let history = unpublished_history(BASE_API).await;
        history.write_commit(path, "fixture\n", "chore: add fixture");
        history.publish_snapshot(&[]);
        history.repo.git(&["rm", path]).unwrap();
        // The walk only counts commits touching a file Cargo packages at that
        // commit, which a deleted file no longer is. Also touch the root
        // lockfile: Cargo packages it, but package equality ignores it.
        let lock = fs_err::read_to_string(history.repo.directory().join("Cargo.lock")).unwrap();
        let breaking = history.write_commit(
            "Cargo.lock",
            &format!("{lock}# changed\n"),
            "feat!: remove fixture",
        );
        let sibling = history.merge_ignored_revert(path, "fixture\n");
        history.assert_release(&[&breaking, &sibling], "0.2.0");
    }
}

#[tokio::test]
async fn nested_metadata_content_changes_do_not_hide_a_retained_package_change() {
    for path in NESTED_METADATA_FILES {
        let history = unpublished_history(BASE_API).await;
        history.write_commit(path, "original\n", "chore: add fixture");
        history.publish_snapshot(&[]);
        fs_err::write(history.repo.directory().join(path), "changed\n").unwrap();
        let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
        let sibling = history.merge_ignored_revert("src/lib.rs", BASE_API);
        history.assert_release(&[&breaking, &sibling], "0.2.0");
    }
}

#[tokio::test]
async fn nested_metadata_deletions_in_the_release_do_not_hide_a_retained_package_change() {
    let path = "src/Cargo.lock";
    let history = api_history(BASE_API).await;
    let fixture = history.write_commit(path, "original\n", "chore: add fixture");
    fs_err::write(history.repo.directory().join(path), "changed\n").unwrap();
    let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    // Undoing the fixture edit cannot restore the fixture the release deleted.
    let sibling = history.merge_ignored_change("src/fix.rs", |root| {
        fs_err::remove_file(root.join(path)).unwrap();
        fs_err::write(root.join("src/lib.rs"), BASE_API).unwrap();
    });
    history.assert_release(&[&fixture, &breaking, &sibling], "0.2.0");
}

#[tokio::test]
async fn nested_metadata_edits_deleted_at_head_are_not_retained() {
    let path = "src/Cargo.lock";
    let history = unpublished_history(BASE_API).await;
    history.write_commit(path, "original\n", "chore: add fixture");
    history.publish_snapshot(&[]);
    fs_err::write(history.repo.directory().join(path), "changed\n").unwrap();
    history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    let sibling = history.merge_ignored_change("src/fix.rs", |root| {
        fs_err::write(root.join(path), "reverted\n").unwrap();
        fs_err::write(root.join("src/lib.rs"), BASE_API).unwrap();
    });
    // Deleting the fixture discards the breaking commit's edit to it.
    history.repo.git(&["rm", path]).unwrap();
    let restore = history.write_commit("src/lib.rs", BASE_API, "fix: restore API");
    history.assert_release(&[&restore, &sibling], "0.1.1");
}

#[tokio::test]
async fn nested_metadata_deletions_are_not_credited_to_content_edits() {
    let path = "src/Cargo.lock";
    let history = api_history(RELEASED_API).await;
    let fixture = history.write_commit(path, "original\n", "chore: add fixture");
    fs_err::write(history.repo.directory().join(path), "changed\n").unwrap();
    history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    let sibling = history.merge_ignored_change("src/fix.rs", |root| {
        fs_err::remove_file(root.join(path)).unwrap();
        fs_err::write(root.join("src/lib.rs"), RELEASED_API).unwrap();
    });
    // HEAD follows the released API but keeps the fixture the release
    // deleted: its addition differs, not the breaking commit's edit.
    let follow = history.write_commit("src/lib.rs", RELEASED_API, "fix: follow the release");
    history.assert_release(&[&fixture, &follow, &sibling], "0.1.1");
}

#[tokio::test]
async fn package_root_metadata_presence_changes_are_not_retained() {
    let history = api_history(BASE_API).await;
    // Package equality ignores the root lockfile entirely, even whether it
    // exists. Untrack it, so that the breaking commit adds it back.
    history
        .repo
        .git(&["rm", "-q", "--cached", "Cargo.lock"])
        .unwrap();
    history
        .repo
        .git(&["commit", "-q", "-m", "chore: remove lockfile"])
        .unwrap();
    history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    let sibling = history.merge_ignored_change("src/fix.rs", |root| {
        fs_err::remove_file(root.join("Cargo.lock")).unwrap();
        fs_err::write(root.join("src/lib.rs"), BASE_API).unwrap();
    });
    // Of the breaking commit's changes, HEAD keeps only the lockfile.
    let restore = history.write_commit("src/lib.rs", BASE_API, "fix: restore API");
    history.assert_release(&[&restore, &sibling], "0.1.1");
}

/// In a `history` equal to the release, add a breaking change and revert it on
/// a merged branch. Return the breaking commit and its sibling.
fn revert_breaking_change(history: &TestContext) -> (String, String) {
    assert_commits(&history.update_history(), &[]);
    let breaking = history.write_commit(
        "src/lib.rs",
        "pub fn temporary() {}\n",
        "feat!: temporary API",
    );
    let sibling = history.merge_ignored_revert("src/lib.rs", "");
    (breaking, sibling)
}

/// libgit2 cannot open a repository with this extension, but the replay only
/// reads its object database, so the reverted change is still retained.
#[tokio::test]
async fn partial_clone_extension_does_not_prevent_updates() {
    let history = api_history("").await;
    history
        .repo
        .git(&["config", "core.repositoryformatversion", "1"])
        .unwrap();
    history
        .repo
        .git(&["config", "extensions.partialClone", "origin"])
        .unwrap();
    let (breaking, sibling) = revert_breaking_change(&history);
    history.assert_release(&[&breaking, &sibling], "0.2.0");
}

/// libgit2 cannot read SHA-256 objects, so a change reverted on a merged
/// branch cannot be checked and ancestry pruning applies, as it did before.
#[tokio::test]
async fn sha256_repositories_fall_back_to_ancestry_pruning() {
    let mut history = unpublished_history("").await;
    // Cargo's SHA-256 fallback omits hidden files; publish the same file set.
    let manifest = fs_err::read_to_string(history.repo.directory().join(CARGO_TOML)).unwrap();
    history.write_commit(
        CARGO_TOML,
        &manifest.replace(
            "[package]",
            "[package]\nexclude = [\".cargo\", \".gitignore\"]",
        ),
        "chore: exclude hidden files",
    );
    history.publish_snapshot(&[]);
    let root = history.repo.directory().to_owned();
    fs_err::remove_dir_all(root.join(".git")).unwrap();
    git_cmd::git_in_dir(
        &root,
        &["init", "--object-format=sha256", "--initial-branch=main"],
    )
    .unwrap();
    history.repo.git(&["config", "user.name", "test"]).unwrap();
    history
        .repo
        .git(&["config", "user.email", "test@example.com"])
        .unwrap();
    history.repo.disable_gpg_signing().unwrap();
    history
        .repo
        .git(&["remote", "add", "origin", &history.gitea.repo_clone_url()])
        .unwrap();
    history.repo.add_all_and_commit("chore: baseline").unwrap();
    history.repo = Repo::new(&root).unwrap();
    assert_eq!(history.repo.current_commit_hash().unwrap().len(), 64);
    let (_breaking, sibling) = revert_breaking_change(&history);
    history.assert_release(&[&sibling], "0.1.1");
}

#[tokio::test]
async fn an_ignored_revert_does_not_hide_surviving_sequential_api_changes() {
    for boundary in ["tag", "published", "missing", "equality"] {
        let history = unpublished_history(BASE_API).await;
        history.publish_with_boundary(boundary);
        // The implementation commit also edits the lockfile. Equality ignores it, so the
        // revert is still an equal snapshot, and undoing the commit on that snapshot
        // must not count the lockfile it restores as a package change.
        let lockfile = history.repo.directory().join("Cargo.lock");
        let contents = fs_err::read_to_string(&lockfile).unwrap();
        fs_err::write(lockfile, format!("{contents}# preparation\n")).unwrap();
        let implementation = history.write_commit(
            "src/lib.rs",
            IMPLEMENTED_API,
            "chore: modify implementation",
        );
        let breaking = history.write_commit(
            "src/lib.rs",
            IMPLEMENTED_BREAKING_API,
            "feat!: breaking API",
        );
        let sibling = history.merge_ignored_revert("src/lib.rs", BASE_API);
        assert_release(
            &history.update_history(),
            &[&implementation, &breaking, &sibling],
            "0.2.0",
            boundary,
        );
    }
}

#[tokio::test]
async fn a_discarded_change_stays_excluded_when_a_sibling_changes_the_same_file() {
    for sequential in [false, true] {
        // Visit the discarded change either before or after the equal snapshot.
        for (discarded_day, discarded_first) in [(2, false), (8, true)] {
            let case = format!("sequential={sequential}, discarded_day={discarded_day}");
            let history = api_history(BASE_API).await;
            let repo = &history.repo;
            repo.git(&["checkout", "-b", "feature"]).unwrap();
            let breaking_api = if sequential {
                IMPLEMENTED_BREAKING_API
            } else {
                BREAKING_API
            };
            if sequential {
                history.write_commit_at(
                    "src/lib.rs",
                    IMPLEMENTED_API,
                    "chore: modify implementation",
                    1,
                );
            }
            let discarded = history.write_commit_at(
                "src/lib.rs",
                breaking_api,
                "feat!: discarded breaking API",
                discarded_day,
            );
            repo.checkout_head().unwrap();
            history.write_commit_at(
                "src/lib.rs",
                &format!("{BASE_API}// temporary\n"),
                "chore: temporary",
                3,
            );
            repo.git_at(
                &["merge", "-s", "ours", "-m", "merge feature", "feature"],
                "2000-01-04T00:00:00 +0000",
            )
            .unwrap();
            let equal = history.write_commit_at("src/lib.rs", BASE_API, "revert: temporary", 5);
            repo.git(&["checkout", "feature"]).unwrap();
            let sibling = history.write_commit_at(
                "src/lib.rs",
                &format!("{breaking_api}pub fn extra() {{}}\n"),
                "fix: sibling",
                9,
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
                format!("{BASE_API}pub fn extra() {{}}\n"),
                "{case}"
            );
            let order = history.walk_order();
            assert_eq!(
                order.find(&discarded).unwrap() < order.find(&equal).unwrap(),
                discarded_first,
                "{case}: {order}"
            );
            assert_release(
                &history.update_history(),
                &[&sibling, &merge],
                "0.1.1",
                &case,
            );
        }
    }
}

#[tokio::test]
async fn later_same_line_edits_preserve_only_surviving_breaking_change_markers() {
    for (breaking_survives, version) in [(false, "0.1.1"), (true, "0.2.0")] {
        let history = api_history(BASE_API).await;
        let implementation = history.write_commit(
            "src/lib.rs",
            IMPLEMENTED_API,
            "chore: modify implementation",
        );
        let breaking = history.write_commit(
            "src/lib.rs",
            IMPLEMENTED_BREAKING_API,
            "feat!: breaking API",
        );
        let sibling = history.merge_ignored_revert("src/lib.rs", BASE_API);
        // Evolve the body on the same line, retaining or restoring the signature.
        // The line-level inverse conflicts in both cases; only one needs a minor bump.
        let kept = if breaking_survives {
            IMPLEMENTED_BREAKING_API
        } else {
            IMPLEMENTED_API
        };
        let evolved = history.write_commit(
            "src/lib.rs",
            &kept.replace("/* implementation */", "/* implementation */ /* evolved */"),
            "fix: evolve implementation",
        );
        let expected: &[&str] = if breaking_survives {
            &[&implementation, &breaking, &sibling, &evolved]
        } else {
            &[&implementation, &sibling, &evolved]
        };
        assert_release(
            &history.update_history(),
            expected,
            version,
            &format!("breaking_survives={breaking_survives}"),
        );
    }
}

/// A merge commit can be an ancestor of the equal snapshot too. Its conflict
/// resolution is undone relative to its first parent, like `git revert -m 1`.
#[tokio::test]
async fn a_merge_commit_whose_resolution_survives_is_retained() {
    let history = api_history(BASE_API).await;
    let repo = &history.repo;
    let baseline = repo.current_commit_hash().unwrap();
    let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    repo.git(&["checkout", "-b", "feature", &baseline]).unwrap();
    history.write_commit(
        "src/lib.rs",
        &BASE_API.replace("api() {}", "api() {} // TODO: take a flag"),
        "chore: plan the flag",
    );
    repo.checkout_head().unwrap();
    assert!(
        repo.git(&["merge", "--no-ff", "--no-commit", "feature"])
            .is_err()
    );
    // The resolution drops the plan and documents the flag on a line of its own,
    // so the plan stays excluded although undoing it conflicts on their shared words.
    let resolved = history.write_commit(
        "src/lib.rs",
        &format!("/// Takes a flag.\n{BREAKING_API}"),
        "merge resolved",
    );
    let sibling = history.merge_ignored_revert("src/lib.rs", BASE_API);
    history.assert_release(&[&breaking, &resolved, &sibling], "0.2.0");
}

#[tokio::test]
async fn conflict_resolution_can_preserve_a_change_reverted_on_another_branch() {
    let history = api_history(BASE_API).await;
    let repo = &history.repo;
    let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    repo.git(&["checkout", "-b", "equal"]).unwrap();
    history.write_commit("src/lib.rs", BASE_API, "revert: breaking API");
    repo.checkout_head().unwrap();
    let prepared = history.write_commit("src/lib.rs", RELEASED_API, "chore: prepare merge");
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
    // Both merges resolve `src/lib.rs` to contents that differ from all their parents.
    history.assert_release(
        &[&breaking, &prepared, &resolved, &sibling, &merge],
        "0.2.0",
    );
}

#[tokio::test]
async fn a_release_side_conflict_keeps_a_breaking_change_absent_from_the_release() {
    let history = api_history(RELEASED_API).await;
    let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    let sibling = history.merge_ignored_revert("src/lib.rs", RELEASED_API);

    // Undoing `api(bool)` to `api()` conflicts with the released `api(u8)`.
    // HEAD still has `api(bool)`, so its breaking-change marker must survive.
    history.assert_release(&[&breaking, &sibling], "0.2.0");
}

#[tokio::test]
async fn head_merge_attributes_do_not_discard_an_unreleased_breaking_change() {
    let history = api_history(RELEASED_API).await;
    let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    let sibling = history.merge_ignored_revert("src/lib.rs", RELEASED_API);
    let attributes = history.write_commit(
        ".gitattributes",
        "src/lib.rs merge=union\n",
        "chore: merge attributes",
    );

    // The union rule at HEAD must not mask the release-side API conflict
    // during the hypothetical revert.
    history.assert_release(&[&breaking, &sibling, &attributes], "0.2.0");
}

#[tokio::test]
async fn local_merge_configuration_does_not_discard_an_unreleased_breaking_change() {
    let union_merge_configurations: [fn(&Repo); 3] = [
        |repo| {
            fs_err::write(
                repo.directory().join(".git/info/attributes"),
                "src/lib.rs merge=union\n",
            )
            .unwrap();
        },
        |repo| {
            let attributes = repo.directory().with_file_name("attributes");
            fs_err::write(&attributes, "src/lib.rs merge=union\n").unwrap();
            repo.git(&["config", "core.attributesFile", attributes.as_str()])
                .unwrap();
        },
        |repo| {
            repo.git(&["config", "merge.default", "union"]).unwrap();
        },
    ];
    for configure in union_merge_configurations {
        let history = api_history(RELEASED_API).await;
        let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
        let sibling = history.merge_ignored_revert("src/lib.rs", RELEASED_API);
        let repo = &history.repo;
        configure(repo);
        let config_path = repo.directory().join(".git/config");
        let config_before = fs_err::read(&config_path).unwrap();

        // These sources still apply to a bare repository with an empty index.
        // A retained-change check must use ordinary text conflicts independently
        // of both the checked-out snapshot and the user's merge configuration.
        history.assert_release(&[&breaking, &sibling], "0.2.0");
        assert_eq!(fs_err::read(&config_path).unwrap(), config_before);
    }
}

#[tokio::test]
async fn an_already_released_breaking_change_is_not_repeated_after_body_edits() {
    for released_api in [
        "api(_: bool) { /* published implementation */ }",
        "api(_: bool) { println!(\"hello\"); }",
        "api(_: bool) { let _enabled: bool = true; }",
        "api(_: bool) { let _predicate = |_: bool| {}; }",
        "api(_: bool) { fn _helper(_: bool) {} }",
    ] {
        let released_api = BREAKING_API.replace("api(_: bool) {}", released_api);
        let history = api_history(&released_api).await;
        history.write_commit("src/lib.rs", BREAKING_API, "feat!: already released API");
        let sibling = history.merge_ignored_revert("src/lib.rs", &released_api);

        // The release already contains the breaking signature. Its later body edit
        // must not make a line-level revert conflict look like an absent signature.
        history.assert_release(&[&sibling], "0.1.1");
    }
}

#[tokio::test]
async fn a_reverted_breaking_change_with_later_body_edits_is_not_released() {
    for fixed_api in [
        "api() { /* fixed implementation */ }",
        "api() { println!(\"hello\"); }",
        "api() {\n    let enabled = true;\n}",
        "api() {\n    let enabled: bool = true;\n}",
        "api() { let enabled: bool = true; }",
        "api() { let count: u8 = 1; }",
        "api() { let predicate = |_: bool| {}; }",
        "api() { fn helper(_: bool) {} }",
        "api() { println!(\"(\"); let predicate = |_: bool| {}; }",
        "api() { /* ( */ fn helper(_: bool) {} }",
    ] {
        let history = api_history(BASE_API).await;
        history.write_commit("src/lib.rs", BREAKING_API, "feat!: temporarily break API");
        let sibling = history.merge_ignored_revert("src/lib.rs", BASE_API);
        let fixed = history.write_commit(
            "src/lib.rs",
            &BASE_API.replace("api() {}", fixed_api),
            "fix: restore compatible API and fix implementation",
        );

        // Body edits conflict with a line-level inverse. Repeated punctuation
        // or a private helper's parameter must not align with the removed parameter.
        history.assert_release(&[&sibling, &fixed], "0.1.1");
    }
}

#[tokio::test]
async fn a_change_reverted_everywhere_is_not_retained_for_a_discarded_release_edit() {
    let released = format!("{BASE_API}// released comment\n");
    let history = api_history(&released).await;
    history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    history.write_commit("src/lib.rs", BASE_API, "fix: revert breaking API");
    // The release reverts the change too and edits next to it. The keep-mine
    // merge discards that edit, so HEAD differs from the release in the changed
    // file without carrying the change.
    let sibling = history.merge_ignored_revert("src/lib.rs", &released);
    history.assert_release(&[&sibling], "0.1.1");
}

#[tokio::test]
async fn outside_paths_do_not_retain_a_reverted_breaking_change() {
    #[cfg(target_os = "linux")]
    use std::os::unix::ffi::OsStrExt as _;
    use std::path::Path;

    for outside in [
        Path::new("outside/plain.txt"),
        // macOS filesystems reject invalid UTF-8 filenames.
        #[cfg(target_os = "linux")]
        Path::new(std::ffi::OsStr::from_bytes(b"outside/\xff.txt")),
    ] {
        let history = member_history(|root| {
            fs_err::create_dir(root.join("outside")).unwrap();
            fs_err::write(root.as_std_path().join(outside), "old\n").unwrap();
        })
        .await;
        history.publish_snapshot(&[]);
        fs_err::write(
            history.repo.directory().as_std_path().join(outside),
            "new\n",
        )
        .unwrap();
        history.write_commit(
            "crates/pkg/src/lib.rs",
            BREAKING_API,
            "feat!: temporarily break API",
        );
        let sibling = history.merge_ignored_change("crates/pkg/src/fix.rs", |root| {
            fs_err::write(root.join("crates/pkg/src/lib.rs"), BASE_API).unwrap();
            fs_err::write(root.as_std_path().join(outside), "old\n").unwrap();
        });
        let fixed = history.write_commit(
            "crates/pkg/src/lib.rs",
            BASE_API,
            "fix: revert breaking API",
        );

        // The CLI only copies UTF-8 paths. Keep the invalid path in historical
        // snapshots, where it must still be excluded from the replay.
        if outside.to_str().is_none() {
            fs_err::remove_file(history.repo.directory().as_std_path().join(outside)).unwrap();
            history
                .repo
                .add_all_and_commit("chore: remove outside file")
                .unwrap();
        }
        // Only the outside edit survives. Its encoding must not decide whether
        // this package retains the breaking-change marker.
        history.assert_release(&[&sibling, &fixed], "0.1.1");
    }
}

#[tokio::test]
async fn later_api_edits_preserve_a_retained_breaking_change_marker() {
    for updated_api in [
        "api(_: u8) {}",
        "api(_: bool) { /* evolved implementation */ }",
        "api(_: bool) { println!(\"hello\"); }",
        "api(_: bool) { let enabled: bool = true; }",
        "api(_: bool) { let predicate = |_: bool| {}; }",
        "api(_: bool) { fn helper(_: bool) {} }",
        "api(_: u8) { let enabled: bool = true; }",
        "api(_: u8) { let predicate = |_: bool| {}; }",
        "api(_: u8) { fn helper(_: bool) {} }",
    ] {
        let history = api_history(BASE_API).await;
        let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
        let sibling = history.merge_ignored_revert("src/lib.rs", BASE_API);
        let evolved = history.write_commit(
            "src/lib.rs",
            &BREAKING_API.replace("api(_: bool) {}", updated_api),
            "chore: evolve API",
        );

        // Undoing the signature after a body edit is clean but changes HEAD;
        // an evolved argument leaves a token conflict. Both retain the marker.
        history.assert_release(&[&breaking, &sibling, &evolved], "0.2.0");
    }
}

#[tokio::test]
async fn a_partly_reverted_breaking_change_keeps_its_marker_after_body_edits() {
    let history = api_history(BASE_API).await;
    let breaking_api = BREAKING_API.replace("stable()", "stable(_: bool)");
    let breaking = history.write_commit("src/lib.rs", &breaking_api, "feat!: break two APIs");
    let sibling = history.merge_ignored_revert("src/lib.rs", BASE_API);
    let fixed_api = BASE_API
        .replace("api() {}", "api() { fn helper(_: bool) {} }")
        .replace("stable()", "stable(_: bool)");
    let fixed = history.write_commit("src/lib.rs", &fixed_api, "chore: restore one API");

    // The first signature is restored despite the private repeated tokens, but
    // undoing the second signature still changes HEAD. Keep the whole marker.
    history.assert_release(&[&breaking, &sibling, &fixed], "0.2.0");
}

#[tokio::test]
async fn a_retained_change_moved_within_a_file_keeps_its_marker() {
    for moved_api in [
        format!("pub mod moved {{ {BREAKING_API} }}\n"),
        format!("pub mod earlier {{ pub fn api() {{}} }}\npub mod moved {{ {BREAKING_API} }}\n"),
    ] {
        let history = api_history(BASE_API).await;
        let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: break API");
        let sibling = history.merge_ignored_revert("src/lib.rs", BASE_API);
        let moved = history.write_commit("src/lib.rs", &moved_api, "chore: move API");

        history.assert_release(&[&breaking, &sibling, &moved], "0.2.0");
    }
}

/// The release contains an evolution of the breaking change, so the tokens of
/// the change's inverse conflict with the release and with HEAD alike.
#[tokio::test]
async fn a_released_evolution_of_a_breaking_change_is_not_repeated() {
    // A change after the release can touch the same file or another one.
    for (after_path, after_contents) in [
        ("src/after.rs", String::new()),
        ("src/lib.rs", format!("{RELEASED_API}pub fn after() {{}}\n")),
    ] {
        let history = api_history(RELEASED_API).await;
        let (late, after) = history.merge_late_branch(RELEASED_API, after_path, &after_contents);

        // Without a tag or published commit, only the equal snapshot bounds the
        // walk, and the late branch reaches the breaking change through another
        // lineage. HEAD has the released evolution: the change is not repeated.
        history.assert_release(&[&after, &late], "0.1.1");
    }
}

#[tokio::test]
async fn a_released_evolution_is_not_repeated_after_independent_release_edits_are_removed() {
    for released in [
        format!("{RELEASED_API}// unrelated released comment\n"),
        RELEASED_API.replace("api(_: u8) {}", "api(_: u8) { /* released body */ }"),
    ] {
        let history = api_history(&released).await;
        let (late, after) = history.merge_late_branch(&released, "src/lib.rs", RELEASED_API);

        // The late branch reaches the original breaking commit, but HEAD has
        // its released evolution. Unrelated removed text in the same file,
        // including the same line, must not repeat that breaking marker.
        history.assert_release(&[&after, &late], "0.1.1");

        // The same proof must follow the API file when HEAD renames it.
        let renamed = history.rename_api_file();
        history.assert_release(&[&after, &late, &renamed], "0.1.1");
    }
}

/// The release changed the token next to the breaking change without taking
/// the change, and HEAD has both.
#[tokio::test]
async fn a_release_edit_next_to_a_retained_change_keeps_its_marker() {
    let history = unpublished_history("pub fn api(_a: i32) {}\n").await;
    history.publish_snapshot(&[("src/lib.rs", "pub fn api(_a: i64) {}\n")]);
    let breaking = history.write_commit(
        "src/lib.rs",
        "pub fn api(_a: i32, _b: u8) {}\n",
        "feat!: add a parameter",
    );
    let sibling = history.merge_ignored_revert("src/lib.rs", "pub fn api(_a: i64) {}\n");
    let adopted = history.write_commit(
        "src/lib.rs",
        "pub fn api(_a: i64, _b: u8) {}\n",
        "chore: adopt the released type",
    );

    // Undoing the parameter conflicts with the release's edit of the adjacent
    // token, and equally at HEAD. HEAD still has the parameter the release lacks.
    history.assert_release(&[&breaking, &sibling, &adopted], "0.2.0");
}

#[tokio::test]
async fn a_retained_api_deletion_keeps_its_breaking_change_marker() {
    for boundary in ["tag", "published", "missing", "equality"] {
        let history = unpublished_history(BASE_API).await;
        history.publish_with_boundary(boundary);
        let breaking =
            history.write_commit("src/lib.rs", "pub fn stable() {}\n", "feat!: remove API");
        let sibling = history.merge_ignored_revert("src/lib.rs", BASE_API);
        history.assert_release(&[&breaking, &sibling], "0.2.0");
    }
}

#[tokio::test]
async fn a_retained_change_can_move_to_a_different_file() {
    let history = api_history(BASE_API).await;
    let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    let sibling = history.merge_ignored_revert("src/lib.rs", BASE_API);
    history
        .repo
        .git(&["mv", "src/lib.rs", "src/api.rs"])
        .unwrap();
    // A file stays at the old path, so the undo conflicts there instead of
    // following a rename.
    let moved = history.write_commit(
        "src/lib.rs",
        "mod api;\npub use api::*;\n",
        "chore: move API",
    );
    history.assert_release(&[&breaking, &sibling, &moved], "0.2.0");
}

#[tokio::test]
async fn a_retained_change_survives_a_rename_at_head() {
    let history = api_history(BASE_API).await;
    let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    let sibling = history.merge_ignored_revert("src/lib.rs", BASE_API);
    // Nothing is left at the old path, so the replay must follow the rename.
    let renamed = history.rename_api_file();
    history.assert_release(&[&breaking, &sibling, &renamed], "0.2.0");
}

#[tokio::test]
async fn a_shallow_clone_prunes_a_change_it_cannot_replay() {
    let history = api_history(BASE_API).await;
    let breaking = history.write_commit("src/lib.rs", BREAKING_API, "feat!: breaking API");
    // Put the breaking change one level deeper than the equal snapshot.
    let later = history.write_commit("src/later.rs", "", "fix: later");
    let sibling = history.merge_ignored_change("src/fix.rs", |root| {
        fs_err::write(root.join("src/lib.rs"), BASE_API).unwrap();
        fs_err::remove_file(root.join("src/later.rs")).unwrap();
    });
    history.assert_release(&[&breaking, &later, &sibling], "0.2.0");

    // Four levels keep the equal snapshot and the breaking change, but not
    // the parent its revert needs. Without evidence, ancestry pruning applies.
    let shallow = history.clone_with("--depth=4", "shallow");
    shallow.assert_release(&[&later, &sibling], "0.1.1");
}

/// The walk checks out only commits that touch the package, so a partial
/// clone keeps missing the blobs other commits give to unrelated files. The
/// replay neither reads nor fetches them.
#[tokio::test]
async fn partial_clones_replay_retained_changes_without_outside_blobs() {
    let history = member_history(|root| {
        fs_err::create_dir(root.join("docs")).unwrap();
        fs_err::write(root.join("docs/x"), "base\n").unwrap();
    })
    .await;
    history.publish_snapshot(&[]);
    let repo = &history.repo;
    // The parent of the breaking change touches only the outside file.
    let outside = history.write_commit("docs/x", "p\n", "docs: outside the package");
    fs_err::write(repo.directory().join("docs/x"), "c\n").unwrap();
    let breaking =
        history.write_commit("crates/pkg/src/lib.rs", BREAKING_API, "feat!: breaking API");
    let sibling = history.merge_ignored_change("crates/pkg/src/fix.rs", |root| {
        fs_err::write(root.join("crates/pkg/src/lib.rs"), BASE_API).unwrap();
        fs_err::write(root.join("docs/x"), "e\n").unwrap();
    });
    history.assert_release(&[&breaking, &sibling], "0.2.0");

    let partial = history.partial_clone();
    let outside_blob = partial
        .repo
        .git(&["rev-parse", &format!("{outside}:docs/x")])
        .unwrap();
    assert!(partial.missing_objects().contains(&outside_blob));
    // Undoing the breaking change merges the outside file of its parent, of
    // the equal snapshot and of HEAD, which all differ, unless the replay
    // stays within the package.
    let trace_path = partial.cargo_target_dir().join("git-trace");
    let mut command = partial.update_command();
    command.env("GIT_TRACE", &trace_path);
    assert_release(
        &partial.check_update(command),
        &[&breaking, &sibling],
        "0.2.0",
        "",
    );
    let trace = fs_err::read_to_string(trace_path).unwrap();
    assert!(
        !trace.contains(&outside_blob),
        "unexpected outside fetch: {trace}"
    );
}

/// A candidate's parent on a branch the release excludes is never checked
/// out, so a partial clone lacks its package blobs. The replay fetches them,
/// the README's before inserting it into the restricted tree, which requires
/// the blob to exist.
#[tokio::test]
async fn partial_clones_fetch_the_package_blobs_a_replay_needs() {
    // Cargo detects the README in the package directory.
    let history = member_history(|root| {
        fs_err::write(root.join("crates/pkg/README.md"), "# base\n").unwrap();
    })
    .await;
    let repo = &history.repo;
    let readme = repo.directory().join("crates/pkg/README.md");
    let baseline = repo.current_commit_hash().unwrap();
    // Both branches edit the API and the README, so only the baseline has
    // their original blobs.
    repo.git(&["checkout", "-b", "feature"]).unwrap();
    fs_err::write(&readme, "# breaking\n").unwrap();
    let breaking =
        history.write_commit("crates/pkg/src/lib.rs", BREAKING_API, "feat!: breaking API");
    repo.checkout_head().unwrap();
    fs_err::write(&readme, "# released\n").unwrap();
    history.write_commit("crates/pkg/src/lib.rs", RELEASED_API, "feat: released API");
    history.run_cargo_publish(PACKAGE);
    // Keep the feature branch's files when merging it into the release.
    assert!(
        repo.git(&["merge", "--no-ff", "--no-commit", "feature"])
            .is_err()
    );
    fs_err::write(&readme, "# breaking\n").unwrap();
    history.write_commit("crates/pkg/src/lib.rs", BREAKING_API, "merge feature");
    let sibling = history.merge_ignored_change("crates/pkg/src/fix.rs", |root| {
        fs_err::write(root.join("crates/pkg/src/lib.rs"), RELEASED_API).unwrap();
        fs_err::write(root.join("crates/pkg/README.md"), "# released\n").unwrap();
    });
    history.assert_release(&[&breaking, &sibling], "0.2.0");

    let partial = history.partial_clone();
    let baseline_blobs = ["crates/pkg/src/lib.rs", "crates/pkg/README.md"].map(|path| {
        partial
            .repo
            .git(&["rev-parse", &format!("{baseline}:{path}")])
            .unwrap()
    });
    let missing = partial.missing_objects();
    assert!(baseline_blobs.iter().all(|blob| missing.contains(blob)));
    let trace_path = partial.cargo_target_dir().join("git-trace");
    let mut command = partial.update_command();
    command.env("GIT_TRACE", &trace_path);
    assert_release(
        &partial.check_update(command),
        &[&breaking, &sibling],
        "0.2.0",
        "",
    );
    // The CLI fetches into its isolated repository, not the user's clone.
    let trace = fs_err::read_to_string(trace_path).unwrap();
    for blob in baseline_blobs {
        assert!(trace.contains(&blob), "missing fetch of {blob}: {trace}");
    }
}

#[tokio::test]
async fn sibling_commits_are_collected_with_tag_published_sha_or_equality_boundary() {
    for boundary in ["tag", "published", "equality"] {
        let history = unpublished_history("").await;
        history.publish_with_boundary(boundary);
        let (one, two) = history.two_merged_siblings();
        assert_commits(&history.update_history(), &[&one, &two]);
    }
}

/// A release from a dirty tree can differ from every committed snapshot.
#[tokio::test]
async fn the_published_commit_bounds_the_walk_without_an_equal_snapshot() {
    for published_boundary in [false, true] {
        let history = unpublished_history("").await;
        let published = history.write_commit("src/released.rs", "", "feat: released");
        let released = "// published from a modified working tree\n";
        if published_boundary {
            fs_err::write(history.repo.directory().join("src/released.rs"), released).unwrap();
            history
                .cargo_publish_command(&history.gitea.repo)
                .arg("--allow-dirty")
                .assert()
                .success();
            history.repo.git(&["restore", "src/released.rs"]).unwrap();
        } else {
            history.publish_snapshot(&[("src/released.rs", released)]);
        }
        let unreleased = history.write_commit("src/unreleased.rs", "", "feat: unreleased");
        let changelog = history.update_history();
        if published_boundary {
            assert_commits(&changelog, &[&unreleased]);
        } else {
            // No local snapshot equals the release, so nothing bounds the walk.
            assert!(
                commit_ids(&changelog).contains(&published.as_str()),
                "{changelog}"
            );
        }
    }
}

/// A history rewrite can remove the published commit; ignore that boundary.
#[tokio::test]
async fn a_published_commit_missing_from_the_repository_is_ignored() {
    let history = unpublished_history("").await;
    history.publish_with_boundary("missing");
    let unreleased = history.write_commit("src/unreleased.rs", "", "feat: unreleased");
    assert_commits(&history.update_history(), &[&unreleased]);
}

#[tokio::test]
async fn late_merge_keeps_mainline_changes_after_the_release() {
    let history = unpublished_history("").await;
    let repo = &history.repo;
    repo.git(&["checkout", "-b", "old-branch"]).unwrap();
    let branch = history.write_commit("src/branch.rs", "", "fix: old branch");
    repo.checkout_head().unwrap();
    history.write_commit("src/released.rs", "", "feat: already released");
    history.run_cargo_publish(&history.gitea.repo);
    repo.tag_lightweight("v0.1.0").unwrap();
    let mainline = history.write_commit("src/mainline.rs", "", "fix: mainline");
    repo.git(&["merge", "--no-ff", "-m", "merge old branch", "old-branch"])
        .unwrap();
    assert_commits(&history.update_history(), &[&branch, &mainline]);
}

#[tokio::test]
async fn final_revert_does_not_release_reverted_changes() {
    let history = api_history("").await;
    history.repo.tag_lightweight("v0.1.0").unwrap();
    history.write_commit("src/lib.rs", "pub fn temporary() {}\n", "feat: temporary");
    history.write_commit("src/lib.rs", "", "revert: temporary");
    assert!(history.update_history().is_empty());
    history.repo.git(&["tag", "-d", "v0.1.0"]).unwrap();
    assert!(history.update_history().is_empty());
}

#[tokio::test]
async fn equal_snapshot_excludes_its_ancestors_but_keeps_sibling_changes() {
    let history = api_history("").await;
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
    assert_commits(&history.update_history(), &[&branch, &sibling]);
}

/// Each branch reverts to the released tree before contributing a fix.
#[tokio::test]
async fn every_lineage_stops_at_its_own_equal_snapshot() {
    let history = api_history("").await;
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
    assert_commits(&history.update_history(), &[&one, &two, &unreleased]);
}

/// History simplification hides one parent of an "ours" merge. Pruning an equal
/// snapshot must still remove that ancestor, whichever one the walk visits first.
#[tokio::test]
async fn discarded_ancestors_are_pruned_in_either_visit_order() {
    for (discarded_day, discarded_first) in [(1, false), (6, true)] {
        let history = api_history("").await;
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
        assert_commits(&history.update_history(), &[&unreleased]);
    }
}

#[tokio::test]
async fn workspace_dependency_updates_are_detected_without_package_commits() {
    for update_lockfile in [false, true] {
        let history = TestContext::new_workspace_with_packages(&[
            TestPackage::new(PACKAGE),
            TestPackage::new("history-dependency").with_type(PackageType::Lib),
        ])
        .await;
        let repo = &history.repo;
        history.run_cargo_publish("history-dependency");
        let manifest = repo.directory().join(CARGO_TOML);
        let contents = fs_err::read_to_string(&manifest).unwrap();
        fs_err::write(&manifest, format!("{contents}\n[workspace.dependencies]\nhistory-dependency = {{ version = \"0.1\", registry = \"test-registry\" }}\n")).unwrap();
        let app = history.package_path(PACKAGE).join(CARGO_TOML);
        let contents = fs_err::read_to_string(&app).unwrap();
        fs_err::write(
            app,
            format!("{contents}\nhistory-dependency.workspace = true\n"),
        )
        .unwrap();
        history.run_cargo_check();
        history.write_release_plz_toml(HISTORY_CONFIG);
        history.run_cargo_publish(PACKAGE);
        repo.tag_lightweight("pkg-v0.1.0").unwrap();
        assert!(history.update_history().is_empty());
        let (path, old, new, expected) = if update_lockfile {
            (
                "crates/history-dependency/Cargo.toml",
                "0.1.0",
                "0.1.1",
                "chore: update Cargo.lock dependencies",
            )
        } else {
            (
                "Cargo.toml",
                "version = \"0.1\"",
                "version = \">=0.1.0\"",
                "chore: update Cargo.toml dependencies",
            )
        };
        let manifest = repo.directory().join(path);
        let contents = fs_err::read_to_string(&manifest).unwrap();
        fs_err::write(manifest, contents.replace(old, new)).unwrap();
        if update_lockfile {
            repo.add_all_and_commit("chore: bump dependency").unwrap();
            history.run_cargo_publish("history-dependency");
            assert_cmd::Command::new("cargo")
                .current_dir(repo.directory())
                .arg("update")
                .assert()
                .success();
        }
        history.run_cargo_check();
        repo.add_all_and_commit("chore: workspace dependencies")
            .unwrap();
        assert!(
            repo.git(&["rev-list", "pkg-v0.1.0..HEAD", "--", "crates/pkg"])
                .unwrap()
                .is_empty()
        );
        let changelog = history.update_history();
        assert_commits(&changelog, &["0000000"]);
        assert!(changelog.contains(expected), "{changelog}");
    }
}

#[tokio::test]
async fn first_release_respects_the_commit_limit() {
    let history = unpublished_history("").await;
    history.write_commit("src/one.rs", "", "fix: one");
    history.write_commit("src/two.rs", "", "fix: two");
    let all = history.walk_order();
    let expected: Vec<_> = all.lines().collect();
    // Zero means no limit. Commits must be collected newest first.
    for (limit, expected) in [(1, &expected[..1]), (2, &expected[..2]), (0, &expected[..])] {
        let mut command = history.update_command();
        command.args(["--max-analyze-commits", &limit.to_string()]);
        let changelog = history.check_update(command);
        assert_eq!(commit_ids(&changelog), expected, "commit limit: {limit}");
    }
}

/// Unpublished packages have only the tag to bound their history.
#[tokio::test]
async fn a_tag_bounds_the_history_of_a_package_that_is_not_published() {
    let history = unpublished_history("").await;
    history.write_release_plz_toml(&HISTORY_CONFIG.replace(
        "semver_check = false",
        "semver_check = false\npublish = false",
    ));
    history.write_commit("src/released.rs", "", "feat: released by the tag");
    history.repo.tag_lightweight("v0.1.0").unwrap();
    let unreleased = history.write_commit("src/unreleased.rs", "", "feat: after the tag");
    assert_commits(&history.update_history(), &[&unreleased]);
}

#[tokio::test]
async fn allow_dirty_preserves_uncommitted_changes() {
    let history = unpublished_history("").await;
    history.write_commit("src/lib.rs", "pub fn one() {}\n", "feat: one");
    history.write_commit("src/lib.rs", "pub fn two() {}\n", "feat: two");
    let lib = history.repo.directory().join("src/lib.rs");
    let dirty = "pub fn dirty() {}\n";
    fs_err::write(&lib, dirty).unwrap();
    let output = history.run_update().failure();
    let error = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(error.contains("uncommitted changes"), "{error}");
    history
        .update_command()
        .arg("--allow-dirty")
        .assert()
        .success();
    assert_eq!(fs_err::read_to_string(lib).unwrap(), dirty);
}

#[tokio::test]
async fn a_tip_matching_the_release_releases_nothing_although_its_branches_differ() {
    let history = unpublished_history("").await;
    history.two_merged_siblings();
    // Neither sibling matches the published tree; their merge does.
    history.publish_snapshot(&[]);
    assert!(history.update_history().is_empty());
}
