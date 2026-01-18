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

## ⚠️ About This Fork

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
| **Use case** | Standard Rust projects | Large private monorepos |

### When to Use k-releaser

Use k-releaser **only** if:
- ✅ You have a large Rust workspace/monorepo
- ✅ You want all workspace packages to share the same version
- ✅ You prefer a single changelog for the entire repository
- ✅ You only use git tags for version tracking

### When to Use release-plz Instead

Use [release-plz](https://github.com/release-plz/release-plz) if:
- ✅ You want independent versioning for workspace packages
- ✅ You prefer separate changelogs per package
- ✅ You're working on a standard Rust project
- ✅ You want the recommended, well-supported tool

### Credits

k-releaser is a heavily modified fork of [release-plz](https://github.com/release-plz/release-plz), created by Marco Ieni. The original release-plz is an excellent tool that serves the Rust ecosystem well. This fork exists solely to address specific edge cases in large private monorepos and should not be seen as a replacement for the original project.

## 🤔 What's a Release PR?

k-releaser maintains Release PRs, keeping them up-to-date as you merge additional commits. When you're
ready to create a release, simply merge the release PR.

![pr](website/docs/assets/pr.png)

When you merge the Release PR (or when you edit the `Cargo.toml` versions by yourself),
k-releaser:

- Creates a git tag named `v<version>` (e.g. `v1.8.1`).
- Publishes a GitHub/Gitea/GitLab release based on the git tag.
- Updates all workspace packages to the same version (unified versioning).

## ⚙️ Configuration

k-releaser is configured in your root `Cargo.toml` file under `[workspace.metadata.k-releaser]` for workspaces or `[package.metadata.k-releaser]` for single packages.

### Basic Configuration

```toml
[workspace.metadata.k-releaser]
# Only create releases when commits match this pattern (optional)
# Useful to skip releases for chore/docs commits
release_commits = "^(feat|fix):"

# Create and update CHANGELOG.md file (default: false)
# Set to true to maintain a changelog file in your repository
changelog_update = true

# Path to custom git-cliff changelog config (optional)
# Defaults to "keep a changelog" format
changelog_config = ".github/cliff.toml"
```

### Version Control

```toml
[workspace.metadata.k-releaser]
# Git tag name template (default: "v{{ version }}")
# Available variables: {{ version }}, {{ package }}
git_tag_name = "v{{ version }}"

# Maximum commits to analyze for first release (default: 1000)
max_analyze_commits = 2000
```

### Git Release Configuration

```toml
[workspace.metadata.k-releaser]
# Enable/disable GitHub/Gitea/GitLab releases (default: true)
git_release_enable = true

# Git release name template (optional)
# Available variables: {{ version }}, {{ package }}
git_release_name = "Release {{ version }}"

# Git release body template (optional)
# Uses changelog by default
git_release_body = "{{ changelog }}"

# Release type: "prod", "pre", or "auto" (default: "prod")
# "auto" marks as pre-release if version contains -rc, -beta, etc.
git_release_type = "auto"

# Create release as draft (default: false)
git_release_draft = false

# Mark release as latest (default: true)
git_release_latest = true
```

### Pull Request Configuration

```toml
[workspace.metadata.k-releaser]
# PR title template (optional)
pr_name = "chore: release {{ version }}"

# PR body template (optional)
# Available variables: {{ changelog }}, {{ version }}, {{ package }}
pr_body = """
## Release {{ version }}

{{ changelog }}
"""

# Create PR as draft (default: false)
pr_draft = false

# Labels to add to PR (optional)
pr_labels = ["release", "automated"]

# PR branch prefix (default: "release-plz-")
pr_branch_prefix = "release-"
```

### Changelog Customization

Advanced changelog customization using git-cliff templates:

```toml
[workspace.metadata.k-releaser.changelog]
# Changelog header
header = """
# Changelog

All notable changes to this project will be documented in this file.
"""

# Changelog entry template (Tera template)
body = """
## [{{ version }}]({{ release_link }}) - {{ timestamp | date(format="%Y-%m-%d") }}

{% for group, commits in commits | group_by(attribute="group") %}
### {{ group | upper_first }}
{% for commit in commits %}
  - {{ commit.message }}{% if commit.breaking %} **BREAKING**{% endif %}
{% endfor %}
{% endfor %}
"""

# Remove leading/trailing whitespace (default: true)
trim = true

# Sort commits: "oldest" or "newest" (default: "newest")
sort_commits = "newest"

# Protect breaking changes from being skipped (default: false)
protect_breaking_commits = true
```

### Repository Settings

```toml
[workspace.metadata.k-releaser]
# Repository URL (defaults to git remote)
# Used for generating changelog links
repo_url = "https://github.com/your-org/your-repo"

# Allow dirty working directory (default: false)
allow_dirty = false

# Update all dependencies in Cargo.lock (default: false)
# If false, only updates workspace packages
dependencies_update = false
```

### Per-Package Overrides

Override settings for specific packages (rarely needed with unified versioning):

```toml
[[workspace.metadata.k-releaser.package]]
name = "my-package"
# Custom changelog path for this package
changelog_path = "packages/my-package/CHANGELOG.md"
```

### Complete Example

```toml
[workspace.metadata.k-releaser]
# Core settings
changelog_update = true
release_commits = "^(feat|fix|perf):"

# Git configuration
git_tag_name = "v{{ version }}"
git_release_name = "{{ version }}"
git_release_type = "auto"

# PR configuration
pr_draft = false
pr_labels = ["release"]
pr_branch_prefix = "release-"

# Repository
repo_url = "https://github.com/your-org/your-repo"
dependencies_update = false
```

## 🤖 Running k-releaser

There are two ways to run k-releaser:

- [CLI](https://k-releaser.dev/docs/usage): Run k-releaser from your terminal or other CI systems (Gitea and GitLab supported).


## 🌓 Related projects

- **[release-plz](https://github.com/release-plz/release-plz)**: The parent project that k-releaser is forked from.
  An excellent tool for automating releases of Rust projects with crates.io publishing, per-package versioning,
  and comprehensive changelog management. **Use this for most Rust projects.**
- [release-please](https://github.com/googleapis/release-please): Both release-plz and k-releaser are inspired by release-please
  and use git tags for version detection. release-please is language-agnostic and widely used across Google's projects.
- [cargo-smart-release](https://github.com/Byron/cargo-smart-release):
  Fearlessly release workspace crates with beautiful semi-handcrafted changelogs.

## 🙏 Credits

k-releaser is a fork of [release-plz](https://github.com/release-plz/release-plz) by Marco Ieni. The majority of the codebase, architecture, and design comes from release-plz.

Additional inspiration and code from:

- [cargo-clone](https://github.com/JanLikar/cargo-clone)
- [cargo-edit](https://github.com/killercup/cargo-edit)
- [cargo-release](https://github.com/crate-ci/cargo-release)
- [cargo-workspaces](https://github.com/pksunkara/cargo-workspaces)
- [git-cliff](https://github.com/orhun/git-cliff)
