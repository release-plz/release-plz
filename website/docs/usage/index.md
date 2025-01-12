# CLI Usage

Release-plz updates the versions and changelogs of your rust packages, by
analyzing your git history.
There are three main commands:

- [`release-plz update`](update.md) updates your project locally, without
  committing any change.
- [`release-plz release-pr`](release-pr.md) opens a GitHub Pull Request.
- [`release-plz release`](release.md) publishes the new versions of the packages.

There are also some utility commands:

- [`release-plz init`](init.md) initializes release-plz for the current GitHub repository.
- [`release-plz set-version`](set-version.md) edits the version of a package in Cargo.toml and changelog.
- [`release-plz generate-completions`](shell-completion.md) generates command completions for
  shells.
- [`release-plz generate-schema`](generate-schema.md) generates the JSON schema for the
  release-plz configuration file.

To learn more about how to use release-plz, run `release-plz --help`.
