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

Here is an example based on release-plz's own
[`cd.yml` workflow](https://github.com/release-plz/release-plz/blob/main/.github/workflows/cd.yml):

>  **Note:** to use this in your project, you would change `"MyOwner"` to your username/organisation
> and the release name from `"my-bin-v"` to the name of your package + `"-v"` (for single crate / workspaces
> containing just one public package, it would just be `"v"`, in accordance with how
> [the `git_tag_name` field](https://release-plz.dev/docs/config#the-git_tag_name-field)).

```yaml
name: CD # Continuous Deployment

permissions:
  contents: write

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
    strategy:
      matrix:
        include:
          - target: aarch64-unknown-linux-gnu
            os: ubuntu-22.04
          - target: aarch64-unknown-linux-musl
            os: ubuntu-22.04
          - target: aarch64-apple-darwin
            os: macos-13
          - target: aarch64-pc-windows-msvc
            os: windows-2022
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-22.04
          - target: x86_64-unknown-linux-musl
            os: ubuntu-22.04
          - target: x86_64-apple-darwin
            os: macos-13
          - target: x86_64-pc-windows-msvc
            os: windows-2022
          - target: x86_64-unknown-freebsd
            os: ubuntu-22.04
    timeout-minutes: 60
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4
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

Some projects to consider for this task:

- [upload-rust-binary-action](https://github.com/taiki-e/upload-rust-binary-action):
  GitHub Action for building and uploading Rust binary to GitHub Releases.
- [cargo-dist](https://crates.io/crates/cargo-dist):
  shippable application packaging for Rust.

:::caution
To release a binary after release, the release-plz GitHub Action needs to
[trigger further workflow runs](../github/token.md).
:::
