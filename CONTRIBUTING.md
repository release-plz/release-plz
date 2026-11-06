# Contribution guidelines

First off, thank you for considering contributing to release-plz.

If your contribution is not straightforward, please first discuss the change you
wish to make by creating a new issue before making the change.

## Reporting issues

Before reporting an issue on the
[issue tracker](https://github.com/release-plz/release-plz/issues),
please check that it has not already been reported by searching for some related
keywords.

## Pull requests

Try to do one pull request per change.

## AI Policy

Read our [AI policy](https://github.com/release-plz/.github/blob/main/AI_POLICY.md).

## Developing

### Useful Commands

- Build and run release version:

  ```shell
  cargo build --release && cargo run --release
  ```

- Run Clippy:

  ```shell
  cargo clippy --all-targets --all-features --workspace
  ```

- Run all tests:

  ```shell
  cargo test --all-features --workspace
  ```

- Check to see if there are code formatting issues

  ```shell
  cargo fmt --all -- --check
  ```

- Format the code in the project

  ```shell
  cargo fmt --all
  ```

## Benchmarks

The benchmarks run locally with [Criterion](https://github.com/criterion-rs/criterion.rs).
They need Rust and Git, and do not require a registry account, Docker, or a hosted
benchmark service. Cargo builds the executables in the optimized bench profile;
the build itself is not timed.

```shell
# End-to-end latency of the release-plz executable.
cargo bench -p release-plz --bench update

# Conventional-commit parsing and next-version calculation.
cargo bench -p next_version --bench next_version

# Manifest updates across independent workspace packages.
cargo bench -p release-plz --bench manifest_updates
```

The `update` suite creates disposable repositories and compares them with local
released workspaces using `--registry-manifest-path`. All dependencies in the
fixtures are local, Cargo runs offline, and semver checks are explicitly disabled.
This measures CLI startup, Git/Cargo operations, package comparison, manifest and
lockfile updates, and changelog rendering with a fixed template. It does not
measure registry downloads, forge requests, publishing, or `cargo-semver-checks`.

| Benchmark | Workload |
| --- | --- |
| `single_noop` | One unchanged package |
| `single_change` | One package with a fix commit |
| `history_100` | One package with 100 fix commits |
| `workspace_10_noop` | Ten unchanged packages |
| `workspace_10_one_change` | One changed package among ten independent packages |
| `workspace_10_dependency_chain` | Fix propagated through ten packages with exact dependencies |

Fixture generation, Git reset/cleanup, and result validation are excluded from
the reported time. Each iteration starts from the original committed input and
checks the resulting versions and changelogs. These are warm-cache benchmarks:
the OS filesystem cache and the fixture's Cargo target directory are retained
between iterations. The working repository is never modified by the benchmarks.

The `next_version` suite measures fix, feature, and breaking-change histories of
10, 100, and 1,000 commits. Input construction and correctness checks happen
outside the timing loop.

The `manifest_updates` suite isolates updating one package version in workspaces
of 10 and 100 independent packages. It includes scanning all manifests for local
dependency requirements, but excludes fixture creation and Cargo metadata. Each
iteration alternates the target version; unrelated manifests stay unchanged.

Pass a name filter to select a case, or `--test` to execute each selected case
once and validate it without statistical measurement. Long-running CLI cases can
exceed Criterion's target measurement time to collect at least ten samples:

```shell
cargo bench -p release-plz --bench update -- single_change
cargo bench -p release-plz --bench update -- --test
cargo bench -p next_version --bench next_version -- --test
```

Criterion stores results and HTML reports under `target/criterion/` (or the
configured Cargo target directory). To compare a change, save a named baseline
before editing the code, then run the same benchmark against it:

```shell
cargo bench -p release-plz --bench update -- --save-baseline before
# Make the change, then rebuild and compare:
cargo bench -p release-plz --bench update -- --baseline before
```

The same baseline options work for `next_version`. Compare runs on the same
machine with the same Rust, Git, Cargo, build settings, and benchmark fixtures.
Avoid concurrent builds and other heavy workloads. Treat small timing changes as
noise until repeated runs confirm them. Adding or changing a fixture requires a
new baseline.

## Glossary

- publish: A crate can be published to crates.io or to a private cargo registry.
- release: The release is the process that goes from updating the version through
  release-plz to publishing the single crate(s).
- package: For our purposes, crate and package are synonyms.
  In the codebase, we prefer to use the word `package` instead of `crate`.

## Integration tests

We use Gitea as a Git server for our integration tests.

- Start the Gitea server with `cd tests && docker compose up`.
- See their OpenAPI documentation at `http://localhost:3000/api/swagger`.

If you don't want to run tests that need docker, you can run `cargo test --no-default-features`.
