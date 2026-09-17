use std::{path::Path, process::Command};

use anyhow::Context as _;
use camino::Utf8Path;
use tracing::{debug, instrument};

use crate::{Repo, cmd, git_in_dir};

impl Repo {
    #[instrument(skip(directory))]
    pub fn init(directory: impl AsRef<Path>) -> Self {
        let directory = Utf8Path::from_path(directory.as_ref()).unwrap();
        git_in_dir(directory, &["init"]).unwrap();

        // configure author
        git_in_dir(directory, &["config", "user.name", "author_name"]).unwrap();
        git_in_dir(directory, &["config", "user.email", "author@example.com"]).unwrap();
        // disable GPG signing for tests
        git_in_dir(directory, &["config", "commit.gpgsign", "false"]).unwrap();

        fs_err::write(directory.join("README.md"), "# my awesome project").unwrap();
        git_in_dir(directory, &["add", "."]).unwrap();
        git_in_dir(directory, &["commit", "-m", "add README"]).unwrap();
        debug!("repo initialized at {:?}", directory);
        let repo = Self::new(directory).unwrap();
        repo.disable_gpg_signing().unwrap();
        repo
    }

    /// Run git with `date` as both author and committer date, so tests control where
    /// a commit lands in a date-ordered walk.
    pub fn git_at(&self, args: &[&str], date: &str) -> anyhow::Result<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(self.directory())
            .args(args)
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .output()
            .with_context(|| format!("error while running git with args `{args:?}`"))?;
        anyhow::ensure!(
            output.status.success(),
            "error while running git with args `{args:?}`: {output:?}"
        );
        cmd::string_from_bytes(output.stdout)
    }
}
