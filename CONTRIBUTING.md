# Contribution guidelines

First off, thank you for considering contributing to k-releaser.

If your contribution is not straightforward, please first discuss the change you
wish to make by creating a new issue before making the change.

## Reporting issues

Before reporting an issue on the
[issue tracker](https://github.com/k-releaser/k-releaser/issues),
please check that it has not already been reported by searching for some related
keywords.

## Pull requests

Try to do one pull request per change.

## Developing

### Set up

This is no different than other Rust projects.

```shell
git clone https://github.com/k-releaser/k-releaser
cd k-releaser
cargo test
```

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

## Glossary

- publish: A crate can be published to crates.io or to a private cargo registry.
- release: The release is the process that goes from updating the version through
  k-releaser to publishing the single crate(s).
- package: For our purposes, crate and package are synonyms.
  In the codebase, we prefer to use the word `package` instead of `crate`.

## Integration tests

We use Gitea as a Git server for our integration tests.

- Start the Gitea server with `cd tests && docker compose up`.
- See their OpenAPI documentation at `http://localhost:3000/api/swagger`.

If you don't want to run tests that need docker, you can run `cargo test --no-default-features`.
