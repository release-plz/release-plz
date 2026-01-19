# Configuration

k-releaser is configured in your root `Cargo.toml` file under `[workspace.metadata.k-releaser]` for workspaces or `[package.metadata.k-releaser]` for single packages.

> **Note**: k-releaser is **command-driven, not config-driven**. Configuration controls *how* commands work (templates, labels, etc.), not *whether* they run. To create a release, run the `release` command. To skip a release, don't run it. Simple!

## Basic Configuration

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

## Version Control

```toml
[workspace.metadata.k-releaser]
# Git tag name template (default: "v{{ version }}")
# Available variables: {{ version }}, {{ package }}
git_tag_name = "v{{ version }}"

# Maximum commits to analyze for first release (default: 1000)
max_analyze_commits = 2000
```

## Git Release Configuration

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

## Pull Request Configuration

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

## Changelog Customization

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

## Repository Settings

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

## Per-Package Overrides

Override settings for specific packages (rarely needed with unified versioning):

```toml
[[workspace.metadata.k-releaser.package]]
name = "my-package"
# Custom changelog path for this package
changelog_path = "packages/my-package/CHANGELOG.md"
```

## Complete Example

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
