# set-version

Edit the version of a package in Cargo.toml and changelog.

Like [`release-plz update`](update.md), this command looks for the changelog of each package
at the [`changelog_path`](../config.md#the-changelog_path-field) and doesn't edit the changelog
of packages with [`changelog_update`](../config.md#the-changelog_update-field) set to `false`.

In the changelog, `set-version` renames the last release of the package,
replacing the current version of the package with the new one, also in the release link.
To support a changelog shared by multiple packages, release titles can start
with the package name, like in the [single changelog](../extra/single-changelog.md#all-packages)
example: `` ## `my-package` - [1.2.3](...) - 2024-05-17 `` or `## my-package - [1.2.3](...) - 2024-05-17`.
Releases of other packages are ignored.

If the last release of a package isn't its current version, or if its changelog doesn't exist,
`set-version` logs a warning and leaves the changelog unchanged.
If a changelog can't be read, `set-version` fails without editing any file.

- In a project containing a single package pass the version you want to set.
  E.g. `release-plz set-version 1.2.3`

- In a workspace with `[workspace.package].version`, pass a version without a package name
  to update the workspace version. E.g. `release-plz set-version 1.2.3`.
  This updates the changelogs of all packages with `version.workspace = true`,
  preserving their version inheritance. Packages with their own version keep it.
  Local dependency requirements and an existing `Cargo.lock` are updated as well.

- In a workspace, specify a version with the syntax `<package_name>@<version>`.
  E.g. `release-plz set-version my_crate@1.2.3`.
  You can also set multiple versions, separated by space.
  E.g. `release-plz set-version crate1@1.2.3 crate2@2.0.0`

:::info
This command is meant to edit the versions of the packages
of your workspace, not the version of your dependencies.
:::

:::tip
You can use this command to quickly update the version of a package in case release-plz didn't
update to the version you intended, e.g.
because you forgot to prefix a commit message with `feat:`.
:::
