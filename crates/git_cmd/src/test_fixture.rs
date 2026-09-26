use std::path::Path;

use camino::Utf8Path;
use tracing::{debug, instrument};

use crate::{Repo, git_in_dir, git_in_dir_with_env};

impl Repo {
    #[instrument(skip(directory))]
    pub fn init(directory: impl AsRef<Path>) -> Self {
        Self::init_with(directory.as_ref(), &["init"])
    }

    /// Like [`Repo::init`], with `git init --object-format=<object_format>`:
    /// `sha1` or `sha256`.
    #[instrument(skip(directory))]
    pub fn init_with_object_format(directory: impl AsRef<Path>, object_format: &str) -> Self {
        let object_format = format!("--object-format={object_format}");
        Self::init_with(directory.as_ref(), &["init", &object_format])
    }

    /// Run the `git init` command `init` in `directory`, then configure an
    /// author, disable signing and create a root commit with a README.
    fn init_with(directory: &Path, init: &[&str]) -> Self {
        let directory = Utf8Path::from_path(directory).unwrap();
        git_in_dir(directory, init).unwrap();

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
        git_in_dir_with_env(
            self.directory(),
            args,
            &[("GIT_AUTHOR_DATE", date), ("GIT_COMMITTER_DATE", date)],
        )
    }
}
