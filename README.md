[![release-plz-logo](website/static/img/release-plz-social-card.png)](https://release-plz.dev)

[![Crates.io](https://img.shields.io/crates/v/release-plz.svg)](https://crates.io/crates/release-plz)
[![CI](https://github.com/release-plz/release-plz/workflows/CI/badge.svg)](https://github.com/release-plz/release-plz/actions)
[![Docker](https://badgen.net/badge/icon/docker?icon=docker&label)](https://hub.docker.com/r/marcoieni/release-plz)

Release-plz helps you release your Rust packages by automating:

- CHANGELOG generation (with [git-cliff](https://git-cliff.org)).
- Creation of GitHub/Gitea/GitLab releases.
- Publishing to a cargo registry (`crates.io` by default).
- Version bumps in `Cargo.toml`.

Release-plz updates your packages with a release Pull Request based on:

- Your git history, following [Conventional commits](https://www.conventionalcommits.org/).
- API breaking changes (detected by [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks)).

## 🤔 What's a Release PR?

Release-plz maintains Release PRs, keeping them up-to-date as you merge additional commits. When you're
ready to create a release, simply merge the release PR.

![pr](website/docs/assets/pr.png)

When you merge the Release PR (or when you edit the `Cargo.toml` versions by yourself),
release-plz:

- Creates a git tag named `<package_name>-v<version>` (e.g. `tokio-v1.8.1`).
- Publishes the package to the cargo registry by running `cargo publish`.
- Publishes a GitHub/Gitea/GitLab release based on the git tag.

## 📚 Docs

Learn how to use release-plz in the [docs](https://release-plz.dev/).

## 🤖 Running release-plz

There are two ways to run release-plz:

- [GitHub Action](https://release-plz.dev/docs/github): Run Release-plz from CI. The action both updates and releases your packages.
- [CLI](https://release-plz.dev/docs/usage): Run release-plz from your terminal or other CI systems (Gitea and GitLab supported).

## 💖 Users

Here you can find the public repositories using the release-plz GitHub action in CI:

- GitHub search [1](https://github.com/search?type=code&q=path%3A*.yml+OR+path%3A*.yaml+MarcoIeni%2Frelease-plz-action%40)
  and [2](https://github.com/search?type=code&q=path%3A*.yml+OR+path%3A*.yaml+release-plz%2Faction%40)
- [Dependency graph](https://github.com/release-plz/action/network/dependents)

## 📽️ RustLab 23 talk

In RustLab 23, I showed how release-plz simplifies releasing Rust packages, why I created it, and what lessons I learned:

[![RustLab 23 talk](https://github.com/release-plz/release-plz/assets/11428655/30e94b65-9077-454d-8ced-6f77d0344f0c)](https://www.youtube.com/watch?v=kXPBVGDkQSs)

## 🌓 Similar projects

- [release-please](https://github.com/googleapis/release-please): release-plz is inspired by release-please,
  but instead of determining the next versions based on git tags, release-plz compares local packages with
  the ones published in the cargo registry.
  Plus, release-plz doesn't need any configuration and is optimized for Rust projects.
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
