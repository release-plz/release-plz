# Releasing binaries

## Why release-plz doesn't release binaries

> Since release-plz already publishes GitHub releases, would it
> make sense for it to build the binaries of the project and publish
> them to the release assets? 🤔

Not really. Releasing binaries requires setting a CI job different
from the one used to run `release-plz release` because:

- `release-plz release` should run once (for example on an `ubuntu` CI image);
- building binaries requires a different CI image for each platform
  (e.g. `ubuntu`, `macos`, `windows`).

Since users have to set up an additional CI job to build binaries, using release-plz
would not be more convenient than using a different tool.
Plus, releasing binaries is a complex task, which is already well-handled by
other tools in the Rust ecosystem.
For these reasons, release-plz doesn't build and release binaries.

The next section explains how to use other tools to build and release binaries after
release-plz released the new version of your project.

## Releasing binaries after release

If you are using release-plz to release your project, you can
run a CI job on the "tag" or "release" events to build and release the binaries.

Here is an example using `upload-rust-binary-action`:

:::info
To use this in your project, change:

- the repository owner from `"MyOwner"` to your username/organisation.
- the release name from `"my-bin-v"` to the release name of your binary according to
  [`git_release_name`](../config.md#the-git_release_name-field).

:::

```yaml
name: CD # Continuous Deployment

on:
  release:
    types: [published]

env:
  CARGO_INCREMENTAL: 0
  CARGO_NET_GIT_FETCH_WITH_CLI: true
  CARGO_NET_RETRY: 10
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1
  RUSTFLAGS: -D warnings
  RUSTUP_MAX_RETRIES: 10

defaults:
  run:
    shell: bash

jobs:
  upload-assets:
    name: ${{ matrix.target }}
    if: github.repository_owner == 'MyOwner' && startsWith(github.event.release.name, 'my-bin-v')
    runs-on: ${{ matrix.os }}
    permissions:
      contents: write
    strategy:
      matrix:
        include:
          - target: aarch64-unknown-linux-gnu
            os: ubuntu-22.04
          - target: aarch64-unknown-linux-musl
            os: ubuntu-22.04
          - target: aarch64-apple-darwin
            os: macos-14
          - target: aarch64-pc-windows-msvc
            os: windows-2022
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-22.04
          - target: x86_64-unknown-linux-musl
            os: ubuntu-22.04
          - target: x86_64-pc-windows-msvc
            os: windows-2022
          - target: x86_64-unknown-freebsd
            os: ubuntu-22.04
    timeout-minutes: 60
    steps:
      - name: Checkout repository
        uses: actions/checkout@v6
        with:
          persist-credentials: false
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - uses: taiki-e/setup-cross-toolchain-action@v1
        with:
          target: ${{ matrix.target }}
        if: startsWith(matrix.os, 'ubuntu') && !contains(matrix.target, '-musl')
      - uses: taiki-e/install-action@v2
        with:
          tool: cross
        if: contains(matrix.target, '-musl')
      - run: echo "RUSTFLAGS=${RUSTFLAGS} -C target-feature=+crt-static" >> "${GITHUB_ENV}"
        if: endsWith(matrix.target, 'windows-msvc')
      - uses: taiki-e/upload-rust-binary-action@v1
        with:
          bin: my-bin
          target: ${{ matrix.target }}
          tar: all
          zip: windows
          token: ${{ secrets.GITHUB_TOKEN }}
```

:::caution
To trigger this workflow on a release event, the release-plz GitHub Action needs to
[trigger further workflow runs](../github/token.md).
:::

## Using cargo-dist with a draft release

Release-plz itself uses [cargo-dist](https://axodotdev.github.io/cargo-dist/) to
build binaries before publishing its GitHub release. See its
[`release-plz.yml`](https://github.com/release-plz/release-plz/blob/main/.github/workflows/release-plz.yml),
[`cd.yml`](https://github.com/release-plz/release-plz/blob/main/.github/workflows/cd.yml),
and [`dist-workspace.toml`](https://github.com/release-plz/release-plz/blob/main/dist-workspace.toml).

The sequence is:

1. Release-plz publishes the crates, pushes tags, and creates a draft GitHub release
   with `git_release_draft = true` and `git_release_latest = false`.
2. The release workflow reads the binary package's tag from release-plz's
   [`releases` output](../github/output.md) and passes it to the reusable CD workflow.
   Draft releases do not trigger GitHub Actions release events. Calling CD after
   release-plz finishes also avoids racing the draft creation on a tag-push event.
3. Cargo-dist builds archives from that tag. Once every build succeeds, CD uploads
   all artifacts, appends cargo-dist's download table to release-plz's changelog and
   contributors, and publishes the draft. Stable releases are marked latest;
   prereleases retain their prerelease status.

The CD workflow is maintained manually (`ci = []` in the dist configuration) to
coordinate the draft release and announcement. Cargo-dist handles the build plan,
binary builds, archives, checksums, and download table for Linux, macOS, and Windows.
Release-plz uses cargo-dist's default archive formats and directory layout:
`.tar.xz` archives contain a directory on Unix, and Windows `.zip` archives contain
the executable at the root. `cargo-binstall` discovers the format and layout using
the configured `release-plz-v<version>` download URL.

If a build or upload fails, the GitHub release stays draft. Maintainers can rerun
the failed jobs or dispatch **CD** with the existing release tag. The publish step
can replace partially uploaded draft assets, but refuses to change an already
published release. The social announcement runs after publication within CD,
because publishing with `GITHUB_TOKEN` does not trigger another release workflow.

## Other tools

Some projects to consider for this task:

- [upload-rust-binary-action](https://github.com/taiki-e/upload-rust-binary-action):
  GitHub Action for building and uploading Rust binary to GitHub Releases.
- [cargo-dist](https://crates.io/crates/cargo-dist):
  shippable application packaging for Rust.
