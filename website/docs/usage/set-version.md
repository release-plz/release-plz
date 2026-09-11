# set-version

Edit the version of a package in Cargo.toml and changelog.

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
