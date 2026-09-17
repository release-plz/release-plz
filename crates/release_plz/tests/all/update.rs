use git_cmd::{Repo, git_in_dir};
use release_plz_core::fs_utils::Utf8TempDir;

use crate::helpers::cmd::release_plz_cmd;

#[test]
fn update_workspace_with_detached_head() {
    update_detached_workspace(None);
}

#[test]
fn update_workspace_with_detached_head_and_explicit_repo_url() {
    update_detached_workspace(Some("https://github.com/test/explicit"));
}

fn update_detached_workspace(repo_url: Option<&str>) {
    let temp_dir = Utf8TempDir::new().unwrap();
    let project_dir = temp_dir.path().join("project");
    let registry_dir = temp_dir.path().join("registry");
    for dir in [&project_dir, &registry_dir] {
        fs_err::create_dir(dir).unwrap();
        fs_err::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = [\"one\", \"two\"]\nresolver = \"3\"\n",
        )
        .unwrap();
        for name in ["one", "two"] {
            let package_dir = dir.join(name);
            fs_err::create_dir_all(package_dir.join("src")).unwrap();
            fs_err::write(
                package_dir.join("Cargo.toml"),
                format!("[package]\nname = {name:?}\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
            )
            .unwrap();
            fs_err::write(package_dir.join("src/lib.rs"), "// Initial release\n").unwrap();
        }
        assert_cmd::Command::new("cargo")
            .current_dir(dir)
            .args(["generate-lockfile", "--offline"])
            .assert()
            .success();
    }
    fs_err::write(
        project_dir.join("release-plz.toml"),
        "[workspace]\nsemver_check = false\n",
    )
    .unwrap();
    for args in [
        vec!["init"],
        vec!["config", "user.name", "Test"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "commit.gpgsign", "false"],
        vec!["add", "."],
        vec!["commit", "-m", "chore: initial release"],
    ] {
        git_in_dir(&project_dir, &args).unwrap();
    }
    let repo = Repo::new(&project_dir).unwrap();
    if repo_url.is_none() {
        repo.git(&["remote", "add", "origin", "https://github.com/test/project"])
            .unwrap();
    }
    // Model a colocated jj repository without requiring jj to be installed in CI.
    repo.git(&["checkout", "--detach"]).unwrap();
    // Both packages must be updated even though comparing the first walks older commits.
    for name in ["one", "two"] {
        fs_err::write(
            project_dir.join(name).join("src/lib.rs"),
            format!("// Fix {name}\n"),
        )
        .unwrap();
        repo.add_all_and_commit(&format!("fix: update {name}"))
            .unwrap();
    }
    let original_commit = repo.current_commit_hash().unwrap();

    let mut cmd = release_plz_cmd(&temp_dir.path().join("target"));
    cmd.current_dir(&project_dir)
        .args(["update", "--registry-manifest-path"])
        .arg(registry_dir.join("Cargo.toml"));
    if let Some(url) = repo_url {
        cmd.args(["--repo-url", url]);
    }
    cmd.assert().success();

    for name in ["one", "two"] {
        let package_dir = project_dir.join(name);
        let manifest = fs_err::read_to_string(package_dir.join("Cargo.toml")).unwrap();
        assert!(manifest.contains("version = \"0.1.1\""), "{manifest}");
        let changelog = fs_err::read_to_string(package_dir.join("CHANGELOG.md")).unwrap();
        assert!(changelog.contains(&format!("update {name}")), "{changelog}");
        let url = repo_url.unwrap_or("https://github.com/test/project");
        assert!(
            changelog.contains(&format!("{url}/compare/{name}-v0.1.0...{name}-v0.1.1")),
            "{changelog}"
        );
    }
    assert_eq!(repo.current_commit_hash().unwrap(), original_commit);
    assert!(repo.git(&["symbolic-ref", "--quiet", "HEAD"]).is_err());
}
