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
