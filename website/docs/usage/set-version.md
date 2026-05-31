# set-version

Edit the version of a package in Cargo.toml and changelog.

- In a project containing a single package pass the version you want to set.
  E.g. `release-plz set-version 1.2.3`

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

## Workflow: correcting a wrong version on an open release PR

`set-version` is most useful **on the release PR branch**, not on `main`. The
intended flow is:

1. Check out the open release PR branch locally.
2. Run `release-plz set-version <pkg>@<correct version>` from the workspace root.
3. Commit the resulting `Cargo.toml` + `CHANGELOG.md` edits and push back to
   the release PR branch.
4. Let the existing release-plz workflow merge the PR as usual.

This way the corrected version travels through the PR + merge path that the
rest of release-plz already understands, instead of being committed straight
to `main`.

:::warning
**Don't run `set-version` on `main` if you have a CI workflow that releases on
push to `main`.** Doing so will commit a fully-formed version bump directly to
`main`, which most release-plz workflows treat as "release this version now".
The release will go out before any PR review, and any pre-release adjustments
(e.g. extra commits you intended to land first) won't make it in.

The release PR branch exists precisely so that the version + changelog edits
can be staged, reviewed, and amended before publication. Use it.
:::

:::warning
`set-version` rewrites the version of the **most recent changelog entry** —
i.e. the `[Unreleased]` section if you've staged release-plz's normal output,
or otherwise the topmost release entry. If you run it on a clean checkout
where the previous release is already at the top of the changelog, it will
edit *that* entry instead of preparing a new one. Always run `set-version` on
a checkout that already has the in-flight release PR's changelog updates
applied (i.e. the PR branch).
:::
