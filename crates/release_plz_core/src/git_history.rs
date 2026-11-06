use std::collections::HashSet;

use anyhow::Context as _;
use git_cmd::Repo;
use tracing::info;

/// Shallow roots reachable from the commit being analyzed. Path-filtered Git logs
/// can hide roots on merged branches, so inspect the entire ancestry here.
pub(crate) struct ShallowHistory {
    head: String,
    boundaries: HashSet<String>,
}

impl ShallowHistory {
    pub(crate) fn new(repo: &Repo) -> anyhow::Result<Self> {
        let head = repo.current_commit_hash()?;
        Self::at_head(repo, head)
    }

    fn at_head(repo: &Repo, head: String) -> anyhow::Result<Self> {
        let path = repo.git(&["rev-parse", "--git-path", "shallow"])?;
        let path = repo.directory().join(path);
        let shallow = match fs_err::read_to_string(path) {
            Ok(shallow) => shallow,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.into()),
        };
        let mut boundaries: HashSet<String> = shallow.lines().map(str::to_owned).collect();
        if !boundaries.is_empty() {
            let roots = repo.git(&["rev-list", "--max-parents=0", &head])?;
            let roots: HashSet<&str> = roots.lines().collect();
            boundaries.retain(|commit| roots.contains(commit.as_str()));
        }
        Ok(Self { head, boundaries })
    }

    pub(crate) fn contains(&self, commit: &str) -> bool {
        self.boundaries.contains(commit)
    }

    /// A baseline excludes its ancestors, but not missing history on another
    /// branch merged after the baseline.
    pub(crate) fn is_complete_since(
        &self,
        repo: &Repo,
        baselines: &[&str],
    ) -> anyhow::Result<bool> {
        if self.boundaries.is_empty() {
            return Ok(true);
        }
        if baselines.is_empty() {
            return Ok(false);
        }
        let mut args = vec!["rev-list", &self.head, "--not"];
        args.extend_from_slice(baselines);
        let commits = repo.git(&args)?;
        Ok(!commits.lines().any(|commit| self.contains(commit)))
    }

    /// Fetch from the configured upstream without moving the branch being
    /// analyzed. Batching avoids a network round trip for every missing parent.
    pub(crate) fn deepen(&self, repo: &Repo) -> anyhow::Result<()> {
        let remote = repo.original_remote();
        let help = "More git history is needed to determine changes. Run `git fetch --unshallow` or set `fetch-depth: 0` in actions/checkout.";
        info!("Fetching more git history from {remote} to determine changes");
        repo.git(&[
            "fetch",
            "--deepen=50",
            "--no-tags",
            "--no-recurse-submodules",
            remote,
            &self.head,
        ])
        .with_context(|| format!("Failed to deepen the shallow repository. {help}"))?;
        // The remote itself may be shallow. A successful fetch does not imply
        // that it supplied any of the missing ancestry.
        let after = Self::at_head(repo, self.head.clone())?;
        anyhow::ensure!(
            after.boundaries != self.boundaries,
            "Fetching did not provide the missing git history. {help}"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use cargo_metadata::camino::Utf8Path;
    use git_cmd::git_in_dir;

    use super::*;

    fn clone_shallow(source: &Utf8Path, destination: &Utf8Path, depth: &str) -> Repo {
        let url = url::Url::from_directory_path(source).unwrap();
        git_in_dir(
            source,
            &[
                "clone",
                "--depth",
                depth,
                url.as_str(),
                destination.as_str(),
            ],
        )
        .unwrap();
        Repo::new(destination).unwrap()
    }

    fn commit(repo: &Repo, name: &str) -> String {
        fs_err::write(repo.directory().join(name), name).unwrap();
        repo.add_all_and_commit(name).unwrap();
        repo.current_commit_hash().unwrap()
    }

    #[test]
    fn shallow_merge_requires_history_from_both_parents() {
        let root = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(root.path()).unwrap();
        let source = root.join("source");
        fs_err::create_dir(&source).unwrap();
        let repo = Repo::init(&source);
        let initial = commit(&repo, "initial");
        repo.checkout_new_branch("side").unwrap();
        commit(&repo, "side-one");
        let side = commit(&repo, "side-two");
        repo.checkout_new_branch("release-line").unwrap();
        repo.git(&["reset", "--hard", &initial]).unwrap();
        let baseline = commit(&repo, "release");
        repo.git(&["merge", "--no-ff", "side", "-m", "merge side"])
            .unwrap();

        let checkout = clone_shallow(&source, &root.join("checkout"), "2");
        let history = ShallowHistory::new(&checkout).unwrap();
        assert!(history.contains(&baseline));
        assert!(history.contains(&side));
        assert!(!history.is_complete_since(&checkout, &[&baseline]).unwrap());
        let head = checkout.current_commit_hash().unwrap();
        history.deepen(&checkout).unwrap();
        assert_eq!(checkout.current_commit_hash().unwrap(), head);
        assert!(
            ShallowHistory::new(&checkout)
                .unwrap()
                .is_complete_since(&checkout, &[&baseline])
                .unwrap()
        );
    }

    #[test]
    fn shallow_upstream_reports_no_progress() {
        let root = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(root.path()).unwrap();
        let source = root.join("source");
        fs_err::create_dir(&source).unwrap();
        let repo = Repo::init(&source);
        commit(&repo, "initial");
        commit(&repo, "latest");
        let upstream = root.join("upstream");
        clone_shallow(&source, &upstream, "1");
        let checkout = clone_shallow(&upstream, &root.join("checkout"), "1");
        let error = ShallowHistory::new(&checkout)
            .unwrap()
            .deepen(&checkout)
            .unwrap_err();
        assert!(format!("{error:#}").contains("Fetching did not provide the missing git history"));
    }

    #[test]
    fn linked_worktree_uses_shared_shallow_boundaries() {
        let root = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(root.path()).unwrap();
        let source = root.join("source");
        fs_err::create_dir(&source).unwrap();
        let repo = Repo::init(&source);
        commit(&repo, "initial");
        let latest = commit(&repo, "latest");
        let checkout = clone_shallow(&source, &root.join("checkout"), "1");
        let worktree_path = root.join("worktree");
        checkout
            .git(&["worktree", "add", "-b", "linked", worktree_path.as_str()])
            .unwrap();
        let worktree = Repo::new(&worktree_path).unwrap();
        let history = ShallowHistory::new(&worktree).unwrap();
        assert!(history.contains(&latest));
        assert!(history.is_complete_since(&worktree, &[&latest]).unwrap());
        history.deepen(&worktree).unwrap();
        assert!(
            ShallowHistory::new(&worktree)
                .unwrap()
                .is_complete_since(&worktree, &[])
                .unwrap()
        );
    }

    #[test]
    fn unrelated_shallow_branch_does_not_require_fetching() {
        let root = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(root.path()).unwrap();
        let source = root.join("source");
        fs_err::create_dir(&source).unwrap();
        let repo = Repo::init(&source);
        commit(&repo, "initial");
        let baseline = commit(&repo, "release");
        repo.checkout_new_branch("other").unwrap();
        commit(&repo, "other");
        let checkout = clone_shallow(&source, &root.join("checkout"), "1");
        // Fetch a separate complete history without deepening the other branch.
        checkout.git(&["fetch", "origin", &baseline]).unwrap();
        checkout
            .git(&["checkout", "-b", "released", &baseline])
            .unwrap();
        let history = ShallowHistory::new(&checkout).unwrap();
        assert!(history.is_complete_since(&checkout, &[]).unwrap());
    }
}
