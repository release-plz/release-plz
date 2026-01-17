[![k-releaser-logo](website/static/img/k-releaser-social-card.png)](https://k-releaser.dev)

[![Crates.io](https://img.shields.io/crates/v/k-releaser.svg)](https://crates.io/crates/k-releaser)
[![CI](https://github.com/k-releaser/k-releaser/workflows/CI/badge.svg)](https://github.com/k-releaser/k-releaser/actions)
[![Docker](https://badgen.net/badge/icon/docker?icon=docker&label)](https://hub.docker.com/r/marcoieni/k-releaser)

k-releaser helps you release your Rust monorepo packages by automating:

- CHANGELOG generation (with [git-cliff](https://git-cliff.org)).
- Creation of GitHub/Gitea/GitLab releases.
- Unified workspace versioning (all packages share the same version).
- Version bumps in `Cargo.toml`.

k-releaser updates your packages with a release Pull Request based on:

- Your git history, following [Conventional commits](https://www.conventionalcommits.org/).
- Git tags for version detection (no crates.io registry dependency).

## 🤔 What's a Release PR?

k-releaser maintains Release PRs, keeping them up-to-date as you merge additional commits. When you're
ready to create a release, simply merge the release PR.

![pr](website/docs/assets/pr.png)

When you merge the Release PR (or when you edit the `Cargo.toml` versions by yourself),
k-releaser:

- Creates a git tag named `v<version>` (e.g. `v1.8.1`).
- Publishes a GitHub/Gitea/GitLab release based on the git tag.
- Updates all workspace packages to the same version (unified versioning).

## 📚 Docs

Learn how to use k-releaser in the [docs](https://k-releaser.dev/).

## 🤖 Running k-releaser

There are two ways to run k-releaser:

- [GitHub Action](https://k-releaser.dev/docs/github): Run k-releaser from CI. The action both updates and releases your packages.
- [CLI](https://k-releaser.dev/docs/usage): Run k-releaser from your terminal or other CI systems (Gitea and GitLab supported).

## 💖 Users

Here you can find the public repositories using the k-releaser GitHub action in CI:

- GitHub search [1](https://github.com/search?type=code&q=path%3A*.yml+OR+path%3A*.yaml+MarcoIeni%2Fk-releaser-action%40)
  and [2](https://github.com/search?type=code&q=path%3A*.yml+OR+path%3A*.yaml+k-releaser%2Faction%40)
- Dependency graph [1](https://github.com/k-releaser/action/network/dependents?package_id=UGFja2FnZS0zMDY0NDU2NDU0)
  and [2](https://github.com/k-releaser/action/network/dependents?package_id=UGFja2FnZS01NTY5MDk1NDUw)

## 📽️ RustLab 23 talk

In RustLab 23, I showed how k-releaser simplifies releasing Rust packages, why I created it, and what lessons I learned:

[![RustLab 23 talk](https://github.com/k-releaser/k-releaser/assets/11428655/30e94b65-9077-454d-8ced-6f77d0344f0c)](https://www.youtube.com/watch?v=kXPBVGDkQSs)

## 🌓 Similar projects

- [release-please](https://github.com/googleapis/release-please): k-releaser is inspired by release-please
  and uses git tags for version detection. k-releaser is specifically optimized for Rust monorepos with
  unified workspace versioning and minimal configuration.
- [cargo-smart-release](https://github.com/Byron/cargo-smart-release):
  Fearlessly release workspace crates and with beautiful semi-handcrafted changelogs.

## 🙏 Credits

Parts of the codebase are inspired by:

- [cargo-clone](https://github.com/JanLikar/cargo-clone)
- [cargo-edit](https://github.com/killercup/cargo-edit)
- [cargo-release](https://github.com/crate-ci/cargo-release)
- [cargo-workspaces](https://github.com/pksunkara/cargo-workspaces)
- [git-cliff](https://github.com/orhun/git-cliff)

<br>

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version 2.0</a>
or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
</sub>
