## TODO

This document describes what needs to be done in this project.

The project is a fork of `release-plz`, a popular tool to automate Github releases for Rust projects.

## The Problem

`release-plz` does not work as needed for large Rust workspaces. We want to change that. 

- It checks crates.io for the latest version -> It should only check the Github tags for the latest version
- It handles all crates in a workspace separatly
    - Separate changelog for each crate -> There should be only one changelog for the whole repo
- It reads changelog files to known what was changed -> Only the git history should be used
- It tries to map the git commits to the crates and extract the changes -> Use only git history to get all changes since the last tagged version
- It creates a pull request with a body that list each crate in the workspace separatly -> It should treat the whole repo as one project
- The pull request body is not in the "keep a changelog" format -> Create the PR with a body in the "keep a changelog" format
- It bumps the verions wrong. -> The git history should be searched for "conventional commit" messages since the last change and bumbed accordingly
- It is able to release to crates.io -> This functionality is not needed and should not be available in the tool

## What needs to be done

- Rename the project in the repo to `k-releaser`
- Remove all functionality to publish to crates.io directly
- Refactor the "Create a PR" step such that it does not care about any crates in the workspace but only takes the "conventional commit" message into account for the next version
- Refactor the "Create a PR" step such that the new version is used to bumb the version in the root `Cargo.toml` and all workspace dependencies to the same version

---

## Implementation Plan

### Phase 1: Rename Project (release-plz → k-releaser)

**Crate names:**
- [ ] Rename `crates/release_plz/` → `crates/k_releaser/`
- [ ] Rename `crates/release_plz_core/` → `crates/k_releaser_core/`
- [ ] Update all Cargo.toml files with new package names
- [ ] Update all imports: `release_plz` → `k_releaser`, `release_plz_core` → `k_releaser_core`

**Configuration:**
- [ ] Rename `release-plz.toml` → `k-releaser.toml`
- [ ] Update config file name references in code (`crates/release_plz/src/config.rs`)

**String constants:**
- [ ] `crates/release_plz_core/src/pr.rs:7-8` - Branch prefixes: `"release-plz-"` → `"k-releaser-"`
- [ ] `crates/release_plz_core/src/pr.rs:43` - PR footer template
- [ ] `crates/release_plz_core/src/command/update/mod.rs:99` - Error messages

**Documentation:**
- [ ] Update all 105+ markdown files in repository
- [ ] Update website documentation in `website/docs/`
- [ ] Update GitHub Actions workflows (`.github/workflows/`)
- [ ] Update README.md
- [ ] Update CLAUDE.md

**Generated files:**
- [ ] Update JSON schema (`.schema/latest.json`)

---

### Phase 2: Remove Crates.io Publishing (~2000 lines)

**Main publishing logic** (`crates/release_plz_core/src/command/release.rs`):
- [ ] Remove `run_cargo_publish()` function (lines 920-949, 1121-1164)
- [ ] Remove `is_package_published()` (lines 661-731)
- [ ] Remove `registry_indexes()` (lines 791-823)
- [ ] Remove `get_cargo_registry()` (lines 825-867)
- [ ] Remove token management: `with_token()`, `find_registry_token()`, `verify_ci_cargo_registry_token()`
- [ ] Remove trusted publishing logic (lines 895-919)
- [ ] Simplify main release loop (lines 654-689) - remove registry checks

**Registry interaction** (`crates/release_plz_core/src/registry_packages.rs`):
- [ ] Remove `get_registry_packages()` (lines 42-92)
- [ ] Remove `download_packages_from_registry()` (lines 94-133)
- [ ] Remove `initialize_registry_package()` (lines 135-168)

**Cargo utilities:**
- [ ] Remove/simplify `crates/cargo_utils/src/registry.rs`
- [ ] Remove `crates/cargo_utils/src/token.rs` (entire file)
- [ ] Remove registry-related functions in `crates/release_plz_core/src/cargo.rs`

**CLI arguments:**
- [ ] Remove registry token arguments from `crates/release_plz/src/args/*.rs`
- [ ] Remove publish-related flags and options

**Configuration:**
- [ ] Remove registry-related config options from `crates/release_plz/src/config.rs`
- [ ] Update config schema generation

**Dependencies:**
- [ ] Remove `crates-index` dependency from Cargo.toml (if no longer needed)

---

### Phase 3: Version Detection (Crates.io → Git Tags Only)

**Main changes** (`crates/release_plz_core/src/command/update/updater.rs`):
- [ ] Remove registry package fetching (lines 74-82) - delete `registry_packages::get_registry_packages()` call
- [ ] Refactor `get_package_diff()` (lines 588-643):
  - Remove registry package comparison logic
  - Use only git tag commits for comparison
  - Extend existing git tag logic (lines 526-543)
- [ ] Update `get_diff()` to use git tags as primary version source
- [ ] Remove `registry_package` parameter passing throughout the update flow

**Version detection logic:**
- [ ] Implement: Find latest git tag matching workspace version pattern (`v{version}`)
- [ ] Get commit hash for that tag
- [ ] Compare local workspace state against that commit
- [ ] If no tag exists, treat as initial version (0.1.0 or 1.0.0)

---

### Phase 4: Unified Workspace Versioning

**Git tag format** (`crates/release_plz_core/src/project.rs`):
- [ ] Modify `git_tag()` (lines 138-144) to always use workspace format: `v{version}`
- [ ] Remove per-package tag template (`{package}-v{version}`)
- [ ] Update tag search/creation logic throughout

**Version calculation** (`crates/release_plz_core/src/command/update/updater.rs`):
- [ ] Force all packages into single version group (leverage lines 144-169)
- [ ] Modify `get_next_version()` (lines 736-768):
  - Always use workspace version
  - Remove per-package version calculation logic
- [ ] Simplify package loop (lines 111-130):
  - Calculate ONE version for workspace
  - Apply same version to all packages

**Workspace version handling:**
- [ ] Extend workspace version logic (lines 70-86, 171-200)
- [ ] Make workspace version mandatory
- [ ] Update `Cargo.toml` version bumping to update workspace.package.version

**Dependency updates:**
- [ ] Update workspace dependency version references
- [ ] Ensure all workspace members use workspace version: `version.workspace = true`

---

### Phase 5: Single Workspace Changelog

**Changelog generation** (`crates/release_plz_core/src/command/update/updater.rs`):
- [ ] Modify `get_changelog()` (lines 905-973):
  - Remove per-package commit filtering
  - Include ALL commits from all packages since last workspace tag
  - Pass workspace context instead of package name
- [ ] Update `calculate_update_result()` (lines 399-420):
  - Use single changelog path at workspace root
  - Generate one changelog for all updates

**Changelog builder** (`crates/release_plz_core/src/changelog.rs`):
- [ ] Verify "Keep a Changelog" format is maintained (lines 16-24) ✓ (already correct)
- [ ] Update `ChangelogBuilder` (lines 276-420) to accept workspace-wide commits
- [ ] Ensure version entry shows all changes across workspace

**Configuration:**
- [ ] Configure default changelog path: `./CHANGELOG.md` (workspace root)
- [ ] Remove per-package changelog_path options (or ignore them)
- [ ] Optionally: Remove `changelog_include` feature (no longer needed for monorepo)

**Commit collection:**
- [ ] Modify commit retrieval to get all commits since last workspace tag
- [ ] Remove package-specific commit filtering
- [ ] Ensure all conventional commits are captured regardless of affected files

---

### Phase 6: PR Creation Updates

**PR template** (`crates/release_plz_core/src/pr.rs`):
- [ ] Update `DEFAULT_PR_BODY_TEMPLATE` (lines 9-43):
  - Remove per-package version list (lines 13-27)
  - Remove per-package semver check section (lines 28-36)
  - Show single workspace version and unified changelog
  - Keep "Keep a Changelog" format
- [ ] Verify PR title logic (lines 99-142) works for workspace mode (lines 138-140) ✓
- [ ] Update PR footer to reference k-releaser

**PR content:**
- [ ] Show single version: `v{version}`
- [ ] Show single consolidated changelog
- [ ] Optionally: List all updated packages without version numbers
- [ ] Include breaking changes summary if any

**PR body example format:**
```markdown
# Release v2.3.0

## Changes

### Features
- Add new feature X
- Implement feature Y

### Bug Fixes
- Fix issue Z

### Breaking Changes
- Changed API in package A

---
🤖 Generated by [k-releaser](https://github.com/...)
```

---

### Phase 7: Testing & Validation

**Unit tests:**
- [ ] Update tests in `crates/release_plz/tests/`
- [ ] Update tests in `crates/release_plz_core/tests/`
- [ ] Remove tests for crates.io publishing
- [ ] Add tests for workspace-level version detection
- [ ] Add tests for unified changelog generation

**Integration tests:**
- [ ] Test with real workspace repositories
- [ ] Verify git tag creation (`v{version}` format)
- [ ] Verify PR creation with unified changelog
- [ ] Test version bumping across all workspace members

**Edge cases:**
- [ ] Empty workspace (no commits since last tag)
- [ ] First release (no existing tags)
- [ ] Multiple breaking changes across different packages
- [ ] Mixed conventional/non-conventional commits

**CI/CD:**
- [ ] Update GitHub Actions workflows
- [ ] Remove cargo-semver-checks if no longer needed (or keep for breaking change detection)
- [ ] Update release workflow

---

### Phase 8: Documentation

**User documentation:**
- [ ] Update README.md with new workflow
- [ ] Update CONTRIBUTING.md
- [ ] Update website docs for monorepo usage
- [ ] Add migration guide from release-plz

**Code documentation:**
- [ ] Update rustdoc comments
- [ ] Update CLAUDE.md with new architecture
- [ ] Document configuration options for k-releaser.toml

---

## Key Files Reference

### Version Detection & Calculation
- `crates/release_plz_core/src/command/update/updater.rs` - Main update logic
- `crates/next_version/src/next_version.rs` - Conventional commit parsing

### Publishing (TO REMOVE)
- `crates/release_plz_core/src/command/release.rs` - Release command
- `crates/release_plz_core/src/registry_packages.rs` - Registry interaction
- `crates/cargo_utils/src/registry.rs` - Registry config
- `crates/cargo_utils/src/token.rs` - Token management

### Changelog
- `crates/release_plz_core/src/changelog.rs` - Changelog generation
- `crates/release_plz_core/src/command/update/changelog_update.rs` - Changelog tracking

### PR Creation
- `crates/release_plz_core/src/pr.rs` - PR templates and formatting
- `crates/release_plz_core/src/command/release_pr/mod.rs` - PR creation logic

### Project Structure
- `crates/release_plz_core/src/project.rs` - Workspace and package discovery
- `crates/release_plz_core/src/command/update/mod.rs` - Package processing

### Configuration
- `crates/release_plz/src/config.rs` - Configuration schema
- `crates/release_plz/src/changelog_config.rs` - Changelog config

---

## Implementation Complexity

**Easy** (leverage existing features):
- ✓ Unified versioning (version groups already exist)
- ✓ Changelog format (already "Keep a Changelog")
- ✓ Conventional commit parsing (already implemented)

**Medium**:
- Renaming (lots of files but straightforward)
- Git tag changes (modify format and lookup logic)
- PR template (modify Tera template)

**Complex** (requires architectural changes):
- Removing crates.io publishing (~2000 lines, deeply integrated)
- Version detection refactor (from registry to tags-only)
- Changelog generation (from per-package to workspace-wide commit collection)

---

## Recommended Implementation Order

1. **Rename the project** - Get it out of the way first
2. **Remove crates.io publishing** - Simplifies codebase for remaining work
3. **Refactor version detection** - Git tags only
4. **Implement unified versioning** - Workspace-level version
5. **Update changelog generation** - Single workspace changelog
6. **Modify PR creation** - New template and format
7. **Testing & validation** - Ensure everything works
8. **Documentation** - Update all docs

