# FAQ

## What packages does release-plz release?

Release-plz can release packages in multiple ways, such as:

- Publishing to a Cargo registry
- Creating git tags
- Creating repository releases

By default, release-plz considers all packages in a workspace for releasing.
The following are not published to a Cargo registry:

- packages with `publish = false` in the `Cargo.toml`.
- [examples](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#examples) that don't
  specify the [`publish`](https://doc.rust-lang.org/cargo/reference/manifest.html#the-publish-field)
  field in their `Cargo.toml` file. To publish them, set this field.

_All_ packages considered by release-plz, even if not published to a registry,
will have:

- A new release pushed to a different release method (such as git tags and
repository releases)
- Their `Cargo.toml` updated to bump the version of a local dependency if needed.

This behavior can be configured for both the workspace and individual packages in
the [configuration](config.md). If you want to check which packages release-plz
will release and how, run `release-plz release --dry-run`.

## Can I edit the release PR before merging it?

Yes, you can edit the release PR as you would do with any other PR.

Here are some reasons why you might want to edit the release PR:

- Edit the changelog: the `CHANGELOG.md` file produced by release-plz is
  a good starting point, but you might want to add more details to it.
  Release-plz populates the corresponding git release description with the new
  changes of the changelog file.
  Note: you don't need to edit the collabsible changelog in the PR description.
- Edit the version: if you forgot to mark a commit as a
  [breaking change](https://www.conventionalcommits.org/en/v1.0.0/#commit-message-with-description-and-breaking-change-footer),
  or if cargo-semver-checks
  [failed](https://github.com/obi1kenobi/cargo-semver-checks#will-cargo-semver-checks-catch-every-semver-violation)
  to detect a breaking change, you can manually edit the version of the package.

## Does the changelog include the commits from the whole repo?

The changelog of each crate includes the commit that changed one of the
files of the crate or one of its dependencies.

## What if a commit doesn't follow the conventional-commits format?

By default, it will be listed under the section `### Other`.
You can customize the changelog format with the
[`[changelog]`](./config.md#the-changelog-section) configuration section.

## How do I know the branch of the release PR?

If you want to commit something to the release-plz pr branch
after releaze-plz workflow, you need to know the name of the branch
of the release PR.
To do so, you can:

- Query the `/pulls` GitHub
  [endpoint](https://docs.github.com/en/free-pro-team@latest/rest/pulls/pulls?apiVersion=2022-11-28#list-pull-requests).
  For example, release-plz does it
  [here](https://github.com/release-plz/release-plz/blob/a92629ed10b8bb42dde426c0f0001aebbb6fa70e/crates/release_plz_core/src/git/backend.rs#L238).
- Use `git tag | grep release-plz`.

If none of these options work for you or you want release-plz to output
the branch in the jobs
[output](https://docs.github.com/en/actions/using-jobs/defining-outputs-for-jobs),
please open an issue.

## Release-plz opens a PR too often

Release-plz opens a PR when any of the files packaged in the crate changes.

To list the files that cargo published to the registry, run:

```sh
cargo package --list
```

To exclude a file from the list (and therefore from the release PR and `release-plz update` changes),
edit the `exclude` and `include`
[fields](https://doc.rust-lang.org/cargo/reference/manifest.html#the-exclude-and-include-fields)
of the `Cargo.toml`.

## Release-plz bumped the version in a way I didn't expect

Release-plz uses the [next_version](https://crates.io/crates/next_version)
crate to determine the next version.
Please read the [documentation](https://docs.rs/next_version/latest/next_version/),
and open an issue if it's not clear enough.
