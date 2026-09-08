# Semver check

Release-plz uses [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks)
to check for API breaking changes in libraries.

The check results are shown in the release Pull Request and in the output of the
`release-plz update` command:

- If the check is skipped, release-plz shows nothing. This happens when the package
  or its previously released version doesn't contain a library.
- If the check is successful, release-plz shows "(✓ API compatible changes)".
- If the check detects incompatibilities, release-plz shows "(⚠️ API breaking changes)",
  with a report of what went wrong.

Example:

![pr](assets/pr.png)

You can configure whether to run `cargo-semver-checks` or not in the
[configuration file](config.md#the-semver_check-field).

## FAQ

## What's an API breaking change?

It is a change that makes the new version of your library
incompatible with the previous one.

For example, renaming a public function of your library is an API breaking change,
because the users of your library will have to update their code to use the new name.

## Will cargo-semver-checks catch every semver violation?

No, it won't — not yet! There are many ways to break semver, and `cargo-semver-checks`
[doesn't yet have lints for all of them](https://github.com/obi1kenobi/cargo-semver-checks/issues/5).
You still need to check for semver violations manually.

## What happens when release-plz detects API breaking changes?

When release-plz detects API breaking changes, it updates the version of the package
with a "major semver Bump". For example, in the image above, you can see that release-plz updated
the `release_plz_core` version from `0.4.21` to `0.5.0`.
In this way, the users of your library know that the new version contains API breaking
changes, and `cargo update` will not update to it automatically.

You can learn more about semver in the [semver website](https://semver.org/)
and in the [cargo book](https://doc.rust-lang.org/cargo/reference/semver.html)
