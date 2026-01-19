# k-releaser

k-releaser helps you release your Rust monorepo packages by automating:

- Changelog generation (with [git-cliff](https://git-cliff.org)).
- Creation of GitHub/Gitea/GitLab releases.
- Unified workspace versioning (all packages share the same version).
- Version bumps in `Cargo.toml`.

k-releaser updates your packages with a release Pull Request based on:

- Your git history, following [Conventional commits](https://www.conventionalcommits.org/).
- Git tags for version detection (no crates.io registry dependency).

## About This Fork

> **Important**: k-releaser is a specialized fork of the brilliant [release-plz](https://github.com/release-plz/release-plz) and is **not suitable for most Rust projects**. If you're working on a standard Rust project or publishing to crates.io, you should use [release-plz](https://github.com/release-plz/release-plz) instead.

### Why This Fork Exists

k-releaser was created to address specific needs of **large mixed language monorepos** that have a crate binary as their center. It makes fundamentally different design choices that are optimized for this narrow use case.

### Key Differences from release-plz

| Feature | release-plz | k-releaser |
|---------|-------------|------------|
| **Version detection** | Checks crates.io for latest versions | Uses only git tags (no registry dependency) |
| **Workspace handling** | Per-package versioning and changelogs | Unified versioning across all workspace packages |
| **Changelog** | Separate changelog per package | Single workspace-level changelog |
| **Publishing** | Publishes to crates.io | No crates.io publishing (git releases only) |
| **PR format** | Lists each package separately | Treats entire workspace as single unit |
| **Use case** | Standard Rust projects | Large mixed language monorepos |

### When to Use k-releaser

Use k-releaser **only** if:
- You have a large Rust workspace/monorepo
- You want all workspace packages to share the same version
- You prefer a single changelog for the entire repository
- You only use git tags for version tracking

### When to Use release-plz Instead

Use [release-plz](https://github.com/release-plz/release-plz) if:
- You want independent versioning for workspace packages
- You prefer separate changelogs per package
- You're working on a standard Rust project
- You want the recommended, well-supported tool

## What's a Release PR?

k-releaser maintains Release PRs, keeping them up-to-date as you merge additional commits. When you're
ready to create a release, simply merge the release PR.

When you merge the Release PR (or when you edit the `Cargo.toml` versions by yourself),
k-releaser:

- Creates a git tag named `v<version>` (e.g. `v1.8.1`).
- Updates all workspace packages to the same version (unified versioning).
- Publishes a GitHub/Gitea/GitLab release based on the git tag (optional second step).

## How k-releaser Works

### Git History is the Source of Truth

k-releaser follows a fundamental principle: **your git commit history is the single source of truth**.

- **Git commits** → analyzed by git-cliff → **generate PR body**
- **PR body** → reviewed/modified by you → **becomes release body**
- **CHANGELOG.md files** (optional) → write-only outputs (never read for releases)

This means:
- Git history determines what goes in releases
- You can review and modify the PR body before merging
- Optional CHANGELOG.md files can be generated for documentation but are not used as input

### Selective Version Bumping

k-releaser follows [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/). It helps to keep the number of new releases as low as possible. Every release is a burden for the user and should only be done, if the product improved from a users perspective. k-releaser distinguishes between:

**Commits that trigger version bumps:**
- `fix:` - Patch version bump (0.1.0 → 0.1.1)
- `feat:` - Minor version bump (0.1.0 → 0.2.0)
- Breaking changes (`!` or `BREAKING CHANGE:`) - Major version bump (0.1.0 → 1.0.0)
- Custom commit types from your config

**Commits that do NOT trigger version bumps:**
- `ci:` - CI/CD changes
- `docs:` - Documentation updates
- `test:` - Test additions or changes
- `chore:` - Maintenance tasks
- `style:` - Code formatting
- `refactor:` - Code restructuring without behavior changes
- `perf:` - Performance improvements

**All commits appear in the changelog**, but only the commits above trigger a new release PR or version bump.

Example:
```bash
# These create a release PR:
git commit -m "feat: add user authentication"
git commit -m "fix: resolve login bug"

# These don't create a release PR (but appear in changelog):
git commit -m "ci: update GitHub Actions workflow"
git commit -m "docs: update README"
git commit -m "test: add unit tests for auth"
```

### Unified Workspace Releases

When all packages in your workspace share the same version, k-releaser creates a **single unified release**:

- **One git tag** for the entire workspace (e.g., `v1.7.0`)
- **One GitHub/Gitea/GitLab release** with title "Version 1.7.0"
- **One release body** containing the PR changelog (not individual package changelogs)
- **All workspace packages** updated to the same version

This ensures your monorepo is released as a cohesive unit, not as separate packages.


## Running k-releaser

k-releaser provides several commands, each with a specific purpose:

### Commands

- **`k-releaser release-pr`** - Create or update a release PR with version bumps and changelog
- **`k-releaser release`** - Create git tags and GitHub/Gitea/GitLab releases (run after merging release PR)
- **`k-releaser publish`** - Publish packages to a cargo registry (if needed)
- **`k-releaser update`** - Update versions and changelogs locally without creating a PR

### Usage

Run k-releaser from your terminal or CI:

```bash
# Create a release PR
k-releaser release-pr

# After merging the PR, create the release
k-releaser release
```

It's recommended to use the corresponding Github Action to run k-releaser.

You find the Action here: [Github Marketspace - k-releaser](https://github.com/marketplace/actions/k-releaser)

Simple Github CI example:

```yaml
name: k-releaser

on:
  push:
    branches:
      - main

jobs:
  # Release unpublished packages.
  k-releaser-release:
    name: k-releaser release
    runs-on: ubuntu-latest
    if: ${{ github.repository_owner == 'secana' }} # Do not run on forks
    permissions:
      contents: write
    steps:
      - &checkout
        name: Checkout repository
        uses: actions/checkout@v6
        with:
          fetch-depth: 0
          persist-credentials: false
      - &install-rust
        name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Run k-releaser
        uses: secana/k-releaser@v1
        with:
          command: release
        env:
          GITHUB_TOKEN: ${{ secrets.K_RELEASER_TOKEN }}

  # Create a PR with the new versions and changelog, preparing the next release.
  k-releaser-pr:
    name: k-releaser PR
    runs-on: ubuntu-latest
    if: ${{ github.repository_owner == 'secana' }} # Do not run on forks
    permissions:
      contents: write
      pull-requests: write
    concurrency:
      group: k-releaser-${{ github.ref }}
      cancel-in-progress: false
    steps:
      - *checkout
      - *install-rust
      - name: Run k-releaser
        uses: secana/k-releaser@v1
        with:
          command: release-pr
        env:
          GITHUB_TOKEN: ${{ secrets.K_RELEASER_TOKEN }}
```

The `K_RELEASER_TOKEN` must be a `GITHUB_TOKEN` with the rights to edit `content` and `pull-requests`. The default token from Github usually lacks this permission.

## Related projects

- **[release-plz](https://github.com/release-plz/release-plz)**: The parent project that k-releaser is forked from.
  An excellent tool for automating releases of Rust projects with crates.io publishing, per-package versioning,
  and comprehensive changelog management. **Use this for most Rust projects.**
- [release-please](https://github.com/googleapis/release-please): Both release-plz and k-releaser are inspired by release-please
  and use git tags for version detection. release-please is language-agnostic and widely used across Google's projects.
- [cargo-smart-release](https://github.com/Byron/cargo-smart-release):
  Fearlessly release workspace crates with beautiful semi-handcrafted changelogs.

## Credits

k-releaser is a fork of [release-plz](https://github.com/release-plz/release-plz) by Marco Ieni. The majority of the codebase, architecture, and design comes from release-plz.
