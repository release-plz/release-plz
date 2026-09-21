# Releasing binaries

Set `dist = true` for each binary package you want release-plz to distribute:

```toml
[[package]]
name = "my-cli"
dist = true
```

For a binary that is not published to a Cargo registry, also set `git_only = true`.
Keep your usual release-plz release workflow. It will create a draft GitHub release
and dispatch the distribution workflow below for each enabled package.
You do not need to run `cargo dist init`, maintain a cargo-dist configuration file,
or let cargo-dist generate a workflow.

## Build prerequisites

Add this profile to the **workspace root `Cargo.toml`**, or to your package's
`Cargo.toml` if it is not in a workspace:

```toml
[profile.dist]
inherits = "release"
lto = "thin"
```

- `inherits = "release"` starts from your release profile's optimized build settings.
  Cargo requires an `inherits` field for custom profiles.
- `lto = "thin"` is the suggested optimization setting. It enables link-time
  optimization across crates with a lower build-time cost than full LTO. It is
  optional; you can keep your own profile settings.

Release-plz requires this profile and never creates or changes it for you.
Commit it before creating the release tag.

The runner also needs **cargo-dist 0.33.0**, with its `dist` executable on `PATH`.
The GitHub action installs it for distribution commands. For direct CLI use,
[install cargo-dist](https://axodotdev.github.io/cargo-dist/book/install.html)
separately. Release-plz fails if it is missing or a different version is installed.

## Distribution workflow

Add `.github/workflows/dist.yml` on your default branch:

```yaml
name: Distribute binaries

on:
  repository_dispatch:
    types: [release-plz-dist]
  workflow_dispatch:
    inputs:
      tag:
        description: Existing draft release tag
        required: true
        type: string

permissions:
  contents: write

concurrency:
  group: release-plz-dist-${{ github.event.client_payload.tag || inputs.tag }}
  cancel-in-progress: false

env:
  GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-22.04, macos-14, windows-2022]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: release-plz/action@v0.5
        with:
          command: dist build

  publish:
    needs: build
    runs-on: ubuntu-22.04
    steps:
      - uses: release-plz/action@v0.5
        with:
          command: dist finalize
```

For `dist build` and `dist finalize`, the action checks out the release tag from
the dispatch event, installs release-plz and the pinned cargo-dist version, and
passes the matrix context to release-plz. The Rust code
uploads archives, checksums, and build manifests directly to the draft release.
The finalizer collects those manifests, generates shell/PowerShell installers
where supported, appends cargo-dist's installation instructions and download
table to the existing changelog, and publishes the release.
There are no upload-artifact/download-artifact steps to configure.
The runner must have Rust and the native build dependencies your application needs.

Each build defaults to the host target reported by `rustc -vV`. To cross-compile,
you can use a `target` field in your existing matrix; the action forwards it
automatically. You still need to install the target's toolchain, linker, and any
system dependencies. Runner labels alone are not interpreted as target triples.
Use one matrix job per distinct target.

GitHub [does not start workflows for draft release events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#release),
so this integration uses an explicit `repository_dispatch` event. It
[works with the ordinary GITHUB_TOKEN](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow)
with `contents: write`. A successful dispatch does not guarantee that a matching
workflow is installed: add the workflow before enabling `dist`.

## Recovery and CLI

A failed build, upload, or installer generation leaves the release in draft.
Rerun failed jobs in the same workflow run to reuse successful builds, or manually
start this workflow with the draft's tag to rebuild the whole matrix.
The finalizer rejects missing matrix jobs, receipts from a different run/commit,
and assets deleted or replaced since a job completed. Keep the concurrency group
and `needs: build`; do not use `always()` or `continue-on-error` to publish failed builds.
If the dispatch itself fails after draft creation, use the manual trigger.
Rerunning `release-plz release` is not a redispatch mechanism.

The action wraps these commands:

```sh
release-plz dist build
release-plz dist finalize
```

Both accept `--tag`, `--manifest-path`, `--config`, `--repo-url`, and `--git-token`.
The tag defaults to the dispatch event and the token defaults to `GITHUB_TOKEN`.
They require `GITHUB_RUN_ID` and a clean checkout at the release tag with tags fetched.
When invoking the binary directly in a matrix, provide its context:

```yaml
- run: release-plz dist build
  env:
    RELEASE_PLZ_DIST_MATRIX: ${{ toJSON(matrix) }}
    RELEASE_PLZ_DIST_JOB_INDEX: ${{ strategy.job-index }}
    RELEASE_PLZ_DIST_JOB_TOTAL: ${{ strategy.job-total }}
```

These variables are supplied internally by the action. Without matrix context,
a direct CLI invocation represents one native build. Run the finalizer in the
same workflow run. An already published release is left unchanged.

## Scope

The integration builds one package per release, including packages containing
multiple binaries, and keeps release-plz's existing registry publication behavior.
It uses cargo-dist 0.33.0 through its CLI and creates its configuration in a temporary
copy of the repository. The original manifests and workflows remain unchanged.
The CLI requires that version to be installed; it does not install tools.
The action installs a prebuilt cargo-dist before invoking the CLI.

Cargo-dist's shell and PowerShell installers are included where supported.
Homebrew, npm, signing, attestations, custom installers, custom cargo-dist settings,
and feature matrices are outside this integration. It uses Cargo's default build
features; `publish_features` configures registry publication only.
Existing cargo-dist configuration is rejected rather than merged. Tags must be
understood by cargo-dist (such as `v1.2.3` and `my-cli-v1.2.3`); opaque custom tag
formats and sharing one tag across multiple distributed packages are unsupported.
Standard native runners are the supported starting point. Cross targets require
user-provided build dependencies, and older Linux compatibility is determined by
the runner you select.

Per-job JSON receipts remain release assets alongside cargo-dist's combined
`dist-manifest.json`. They contain the run ID, commit, target and artifact metadata,
and allow failed jobs to be retried without an Actions artifact store.
For more advanced packaging, use cargo-dist independently as described below.

## Releasing binaries after release

If you are using release-plz to release your project, you can
run a CI job on the "tag" or "release" events to build and release the binaries.

Here is an example based on release-plz's own
[`cd.yml` workflow](https://github.com/release-plz/release-plz/blob/main/.github/workflows/cd.yml):

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

Some projects to consider for this task:

- [upload-rust-binary-action](https://github.com/taiki-e/upload-rust-binary-action):
  GitHub Action for building and uploading Rust binary to GitHub Releases.
- [cargo-dist](https://crates.io/crates/cargo-dist):
  shippable application packaging for Rust.

:::caution
To release a binary after release, the release-plz GitHub Action needs to
[trigger further workflow runs](../github/token.md).
:::
