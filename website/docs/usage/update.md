# update

The `release-plz update` command updates the version and the changelog of the
packages containing unreleased changes.

The command:

- Downloads the packages of the project from the cargo registry.
- Compares the local packages with the downloaded ones to determine the new commits.
- Checks for API breaking changes in libraries if
  [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks)
  is installed.
  _Warning:_ `cargo-semver-checks` doesn't catch every semver violation.
- Updates the packages versions based on the messages of the new commits (based
  on [conventional commits](https://www.conventionalcommits.org/) and
  [semantic versioning](https://semver.org/)).
- Updates the packages changelogs with the messages of the new commits.
- Updates all dependencies by running `cargo update` (disabled by default).

In the following example, I run `release-plz` on the `release-plz` project itself.
`Release-plz` increases the version and the changelog of the packages with
unpublished changes.

![release-plz update](https://user-images.githubusercontent.com/11428655/160762832-54300ddb-ec9c-4538-a611-c66490c47333.gif)

To learn more, run `release-plz update --help`.

## Shallow clones

`release-plz update` and `release-plz release-pr` can use shallow clones when
the available history is sufficient to determine the changes for each package.
If more history is needed, release-plz fetches it from the configured upstream
remote in batches and retries the calculation. It also checks for missing history
on merged branches. In `git_only` mode, a missing release tag triggers a tag fetch
before the package is treated as an initial release.

Automatic fetching uses your Git credentials. If fetching fails or the remote
cannot provide more history, release-plz returns an error before updating versions
or changelogs. You can fetch the history yourself with `git fetch --unshallow`,
or configure `actions/checkout` with `fetch-depth: 0`.
