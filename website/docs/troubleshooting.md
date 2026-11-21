# Troubleshooting

## Release-plz GitHub action started misbehaving

> Did your release-plz GitHub action started misbehaving after a [Release-plz](https://github.com/release-plz/release-plz/releases)
or [GitHub action](https://github.com/release-plz/action/releases) release?

If yes, try to:

- *Pin a specific version of the release-plz GitHub action* in your workflow file.
  E.g. go from `release-plz/action@v0.5` to `release-plz/action@v0.5.16`.
  Determine the right version to pin by looking at the previous GitHub Action
  [releases](https://github.com/release-plz/action/releases)

- *Pin a specific version of the release-plz* in the GitHub action, by specifying the `version` field
  in the GitHub Action [input](./github/input.md).
  E.g. `version: "0.3.70"`.
  The default is the latest version of release-plz.
  Determine the right version to pin by looking at the previous release-plz
  [releases](https://github.com/release-plz/release-plz/releases)

Please open an [issue](https://github.com/release-plz/release-plz/issues), too.

## GitHub rejects branch creation with `403 Resource not accessible by integration`

First, check that the token has `contents: write` and `pull-requests: write`
permissions, as described in the [GitHub setup guide](./github/quickstart.md).

This error can also occur with those permissions when release-plz runs on an older
commit and newer commits have changed `.github/workflows/`. GitHub checks workflow
permissions when creating Git references, even if release-plz itself only changes
versions and changelogs. The default `GITHUB_TOKEN` cannot be granted
`workflows: write`. See this [GitHub discussion](https://github.com/orgs/community/discussions/151442)
for a reproduction of the workflow-file restriction.

Run release-plz on the latest commit of the target branch. Rerunning an old GitHub
Actions run uses its original commit, so it can keep failing even after a newer run
has succeeded. This can also happen when another push advances the branch while a
release-plz job is running; serializing release jobs does not prevent new pushes.

If you need to run on that older commit, use a [GitHub App or personal access token](./github/token.md)
with permission to write workflows as well as the usual release-plz permissions.
Repository rulesets can also restrict branch creation, so check those if the error
persists on the latest commit.

When updating an existing release PR fails with an HTTP `401` or `403`, release-plz
leaves it open and reports the error. Closing it before creating a replacement can
leave you without a release PR if the replacement is denied too.

## `release-plz release` hangs

Something similar happened in [#1015](https://github.com/release-plz/release-plz/issues/1015).
Try to set a low [`publish_timeout`](./config.md#the-publish_timeout-field)
in your `release-plz.toml` file to check if release-plz
is having issues to:

- check if a package was published.
- publish a package.

## See `DEBUG` logs

By default, Release-plz shows logs at the `INFO` level.

The `--verbose` (`-v`) flag can be used to increase the verbosity of the logs.
It can be specified multiple times to increase the verbosity even more.

```bash
release-plz --verbose # (or -v) Make logs verbose
release-plz -vv # Show verbose DEBUG logs
release-plz -vvv # Show verbose TRACE logs
```

Release-plz log level can also be configured using the `RELEASE_PLZ_LOG` environment variable.

```bash
RELEASE_PLZ_LOG=DEBUG release-plz
RELEASE_PLZ_LOG=TRACE release-plz
```
