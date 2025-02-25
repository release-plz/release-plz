# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.33.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.32.7...release_plz_core-v0.33.0) - 2025-02-25

### Added

- edit changelog order to respect release order (#2070)

### Other

- split next_ver module (#2095)

## [0.32.7](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.32.6...release_plz_core-v0.32.7) - 2025-02-22

### Fixed

- don't error if package already published (#2085)

### Other

- use edition 2024 (#2086)

## [0.32.6](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.32.5...release_plz_core-v0.32.6) - 2025-02-22

### Added

- support rust 2024 edition (#2084)

### Fixed

- clippy lint (#2082)
- support env var override for registry index (#2033)

## [0.32.5](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.32.4...release_plz_core-v0.32.5) - 2025-02-09

### Fixed

- Bring back breaking change description in pr body (#2055)
- Show previous version in PR body (#2054)
- Check semver breaking changes on packages containing both library and binary (#2053)
- Show in pr body when semver check passed ([#2057](https://github.com/release-plz/release-plz/pull/2057))

### Other

- hint to install cargo semver checks to run tests (#2056)
- remove direct dipendency on lazy_static (#2049)

## [0.32.4](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.32.3...release_plz_core-v0.32.4) - 2025-02-09

### Fixed

- Don't fail if you can't fetch associated PRs from the remote because the commit hasn't been pushed (#2045)

## [0.32.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.32.2...release_plz_core-v0.32.3) - 2025-02-08

### Fixed

- correctly detect if package is executable (#2041)

## [0.32.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.32.1...release_plz_core-v0.32.2) - 2025-02-08

### Added

- add releases to release-pr json output (#2031)

### Fixed

- don't fail if commit wasn't pushed to remote (#2037)
- correctly detect if package contains executable (#2024)

## [0.32.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.32.0...release_plz_core-v0.32.1) - 2025-02-01

### Fixed

- don't update libraries versions on Cargo.lock update (#2016)

## [0.32.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.31.1...release_plz_core-v0.32.0) - 2025-02-01

### Added

- Download packages from different registries in parallel to improve performance (#2005)

### Fixed

- Don't try to fetch information from the remote if it's a "filler" commit (#2014)

### Other

- ♻️ refactor: use tera template for default PR body ([#1861](https://github.com/release-plz/release-plz/pull/1861))

## [0.31.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.31.0...release_plz_core-v0.31.1) - 2025-01-23

### Fixed

- don't suppress git error when determining commits to release (#1962)
- *(core)* Stash uncommited changes in dirty repos during update (#1982)
- *(core)* Make `release --dry-run` more informative (#1983)

### Other

- allow fetching latest package summaries from cargo registry ([#1984](https://github.com/release-plz/release-plz/pull/1984))

## [0.31.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.30.13...release_plz_core-v0.31.0) - 2025-01-14

### Added

- allow adding labels to Gitea release PR (#1937)

### Fixed

- use dunce to canonicalize package paths (#1975)
- Fix `release-plz init` local dependency check (#1978)

### Other

- refactor to remove duplicate string (#1964)

## [0.30.13](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.30.12...release_plz_core-v0.30.13) - 2024-12-19

### Fixed

- increment pre-release version correctly when dependencies change (#1915)
- Fixed wrong link on github token usage for release ci. (#1913)

### Other

- log when a release is skipped because of release_commits regex (#1904)

## [0.30.12](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.30.11...release_plz_core-v0.30.12) - 2024-12-07

### Added

- warn if cargo-semver-checks is not installed ([#1891](https://github.com/release-plz/release-plz/pull/1891))
- Enable `pr_labels` for GitLab backend ([#1879](https://github.com/release-plz/release-plz/pull/1879))
- add raw_message to commit context ([#1867](https://github.com/release-plz/release-plz/pull/1867))

### Fixed

- locate Cargo.lock correctly when project isn't in root directory ([#1894](https://github.com/release-plz/release-plz/pull/1894))

## [0.30.11](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.30.10...release_plz_core-v0.30.11) - 2024-11-21

### Added

- *(init)* check if Cargo.toml constains the `version` field in local dependencies ([#1843](https://github.com/release-plz/release-plz/pull/1843))

## [0.30.10](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.30.9...release_plz_core-v0.30.10) - 2024-11-21

### Fixed

- don't update changelog if version is already present ([#1834](https://github.com/release-plz/release-plz/pull/1834))

### Other

- move `MarcoIeni/release-plz` to `release-plz/release-plz` ([#1850](https://github.com/release-plz/release-plz/pull/1850))
- migrate website to release-plz.dev ([#1842](https://github.com/release-plz/release-plz/pull/1842))

## [0.30.9](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.30.8...release_plz_core-v0.30.9) - 2024-11-11

### Added

- *(init)*: add check for mandatory fields in Cargo.toml ([#1769](https://github.com/release-plz/release-plz/pull/1769))

## [0.30.8](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.30.7...release_plz_core-v0.30.8) - 2024-11-08

### Added

- add `publish_all_features` config option to workspace/package config ([#1818](https://github.com/release-plz/release-plz/pull/1818))

## [0.30.7](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.30.6...release_plz_core-v0.30.7) - 2024-10-30

### Fixed

- Correctly deserialize GitLab MR commits ([#1808](https://github.com/release-plz/release-plz/pull/1808))

## [0.30.6](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.30.5...release_plz_core-v0.30.6) - 2024-10-27

### Fixed

- revert Cargo.lock changes after running `cargo package` ([#1803](https://github.com/release-plz/release-plz/pull/1803))

## [0.30.5](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.30.4...release_plz_core-v0.30.5) - 2024-10-25

### Fixed

- release squashed release PRs from main branch ([#1801](https://github.com/release-plz/release-plz/pull/1801))

## [0.30.4](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.30.3...release_plz_core-v0.30.4) - 2024-10-18

### Added

- add breaking_changes field to pr_body context ([#1787](https://github.com/release-plz/release-plz/pull/1787))
- add pr_body field ([#1778](https://github.com/release-plz/release-plz/pull/1778))

## [0.30.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.30.2...release_plz_core-v0.30.3) - 2024-10-16

### Fixed

- fix release for squash commits ([#1775](https://github.com/release-plz/release-plz/pull/1775))

## [0.30.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.30.1...release_plz_core-v0.30.2) - 2024-10-15

### Fixed

- checkout on release commit ([#1761](https://github.com/release-plz/release-plz/pull/1761))

## [0.30.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.30.0...release_plz_core-v0.30.1) - 2024-10-13

### Added

- add `pr_name` config field ([#1765](https://github.com/release-plz/release-plz/pull/1765))
- add `remote.pr_number` to `remote` context of commits in changelog ([#1768](https://github.com/release-plz/release-plz/pull/1768))

## [0.30.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.29.0...release_plz_core-v0.30.0) - 2024-10-08

### Added

- add `pr_branch_prefix` option to configure branch prefix for release-pr ([#1728](https://github.com/release-plz/release-plz/pull/1728)) ([#1737](https://github.com/release-plz/release-plz/pull/1737))

## [0.29.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.28.0...release_plz_core-v0.29.0) - 2024-10-05

### Added

- add contributors to git release body context ([#1736](https://github.com/release-plz/release-plz/pull/1736))

### Other

- fix `unnecessary_lazy_evaluations`lint ([#1734](https://github.com/release-plz/release-plz/pull/1734))
- fix `too_many_arguments` lint ([#1733](https://github.com/release-plz/release-plz/pull/1733))

## [0.28.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.27.2...release_plz_core-v0.28.0) - 2024-10-03

### Added

- add contributors' username to changelog context ([#1731](https://github.com/release-plz/release-plz/pull/1731))
- add remote contributor username to commit context of changelog ([#1635](https://github.com/release-plz/release-plz/pull/1635))

## [0.27.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.27.1...release_plz_core-v0.27.2) - 2024-10-01

### Fixed

- changelogs only include commits that edit packaged files ([#1703](https://github.com/release-plz/release-plz/pull/1703))

## [0.27.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.27.0...release_plz_core-v0.27.1) - 2024-09-30

### Fixed

- Disable changelog in PR body if it goes over github character limit ([#1716](https://github.com/release-plz/release-plz/pull/1716))
- retrieve token for each package's registry ala `cargo:token` ([#1669](https://github.com/release-plz/release-plz/pull/1669))

## [0.27.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.26.2...release_plz_core-v0.27.0) - 2024-09-28

### Added

- Add `features_always_increment_minor` flag ([#1657](https://github.com/release-plz/release-plz/pull/1657))

### Other

- update to secrecy 10 ([#1719](https://github.com/release-plz/release-plz/pull/1719))
- update to git-cliff 2.6 ([#1718](https://github.com/release-plz/release-plz/pull/1718))

## [0.26.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.26.1...release_plz_core-v0.26.2) - 2024-09-17

### Added

- support local dependencies in workspace manifest ([#1695](https://github.com/release-plz/release-plz/pull/1695))

### Other

- improve `dependencies_to_update` fn ([#1694](https://github.com/release-plz/release-plz/pull/1694))
- add comments to package update code ([#1692](https://github.com/release-plz/release-plz/pull/1692))
- extract function from `packages_to_update` ([#1691](https://github.com/release-plz/release-plz/pull/1691))
- improve `dependent_packages` function ([#1689](https://github.com/release-plz/release-plz/pull/1689))
- improve dependencies_to_update function ([#1687](https://github.com/release-plz/release-plz/pull/1687))

## [0.26.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.26.0...release_plz_core-v0.26.1) - 2024-09-16

### Fixed

- restore semver check report ([#1684](https://github.com/release-plz/release-plz/pull/1684))

## [0.26.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.25.7...release_plz_core-v0.26.0) - 2024-09-16

### Added

- allow specifying `version_group` field in package config ([#1661](https://github.com/release-plz/release-plz/pull/1661))

## [0.25.7](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.25.6...release_plz_core-v0.25.7) - 2024-09-14

### Fixed

- fallback to http 1.1 when http2 is not supported on fetching sparse metadata ([#1676](https://github.com/release-plz/release-plz/pull/1676))

## [0.25.6](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.25.5...release_plz_core-v0.25.6) - 2024-09-07

### Fixed
- align changelog line breaks with keepachangelog spec ([#1653](https://github.com/release-plz/release-plz/pull/1653))

## [0.25.5](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.25.4...release_plz_core-v0.25.5) - 2024-09-07

### Added
- add gitlab support for the `release-pr` command ([#1651](https://github.com/release-plz/release-plz/pull/1651))

### Other
- fix inspect_err clippy lint ([#1658](https://github.com/release-plz/release-plz/pull/1658))

## [0.25.4](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.25.3...release_plz_core-v0.25.4) - 2024-08-25

### Other
- update dependencies ([#1632](https://github.com/release-plz/release-plz/pull/1632))

## [0.25.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.25.2...release_plz_core-v0.25.3) - 2024-08-18

### Added
- Add committer and author to changelog ([#1606](https://github.com/release-plz/release-plz/pull/1606))
- Add version of the packages into PR name if they all use the same ([#1603](https://github.com/release-plz/release-plz/pull/1603))

## [0.25.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.25.1...release_plz_core-v0.25.2) - 2024-08-07

### Other
- extract Project struct into its own file ([#1597](https://github.com/release-plz/release-plz/pull/1597))

## [0.25.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.25.0...release_plz_core-v0.25.1) - 2024-08-07

### Fixed
- Fixed issue preventing creating GitHub releases ([#1594](https://github.com/release-plz/release-plz/pull/1594))

## [0.25.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.24.4...release_plz_core-v0.25.0) - 2024-08-05

### Added
- add `git_release_latest` configuration option ([#1588](https://github.com/release-plz/release-plz/pull/1588))

## [0.24.4](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.24.3...release_plz_core-v0.24.4) - 2024-08-04

### Added
- *(changelog)* add `remote` to the changelog context ([#1575](https://github.com/release-plz/release-plz/pull/1575))

### Other
- extract `get_changelog` function ([#1574](https://github.com/release-plz/release-plz/pull/1574))
- *(deps)* bump gix-attributes from 0.22.2 to 0.22.3 ([#1573](https://github.com/release-plz/release-plz/pull/1573))

## [0.24.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.24.2...release_plz_core-v0.24.3) - 2024-07-18

### Fixed
- *(release)* specify package name ([#1560](https://github.com/release-plz/release-plz/pull/1560))

## [0.24.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.24.1...release_plz_core-v0.24.2) - 2024-07-14

### Fixed
- *(release)* don't try to parse changelog if it doesn't exist ([#1563](https://github.com/release-plz/release-plz/pull/1563))
- *(release)* set changelog path ([#1561](https://github.com/release-plz/release-plz/pull/1561))

## [0.24.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.24.0...release_plz_core-v0.24.1) - 2024-07-14

### Added
- *(set-version)* support single crate project ([#1553](https://github.com/release-plz/release-plz/pull/1553))

## [0.24.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.23.1...release_plz_core-v0.24.0) - 2024-07-12

### Added
- implement set-version command ([#1546](https://github.com/release-plz/release-plz/pull/1546)) ([#1550](https://github.com/release-plz/release-plz/pull/1550))
- add release_link to context ([#1502](https://github.com/release-plz/release-plz/pull/1502))

### Other
- add missing READMEs ([#1551](https://github.com/release-plz/release-plz/pull/1551))

## [0.23.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.23.0...release_plz_core-v0.23.1) - 2024-06-30

### Other
- update git-cliff to 2.4.0 ([#1535](https://github.com/release-plz/release-plz/pull/1535))

## [0.23.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.22.4...release_plz_core-v0.23.0) - 2024-06-21

### Added
- Support for GitLab on-premise, multilevel URLs and SSH git repos ([#1521](https://github.com/release-plz/release-plz/pull/1521))

### Other
- set GitHub and Gitea tokens to be sensitive ([#1529](https://github.com/release-plz/release-plz/pull/1529))

## [0.22.4](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.22.3...release_plz_core-v0.22.4) - 2024-06-18

### Other
- remove dead code ([#1525](https://github.com/release-plz/release-plz/pull/1525))

## [0.22.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.22.2...release_plz_core-v0.22.3) - 2024-06-10

### Fixed
- fix([#1487](https://github.com/release-plz/release-plz/pull/1487)): don't close release PR on force-push ([#1513](https://github.com/release-plz/release-plz/pull/1513))

## [0.22.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.22.1...release_plz_core-v0.22.2) - 2024-06-09

### Fixed
- Handle CARGO_REGISTRIES_<name>_TOKEN env with uppercase ([#1507](https://github.com/release-plz/release-plz/pull/1507))
- readme comparison ([#1496](https://github.com/release-plz/release-plz/pull/1496))

## [0.22.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.22.0...release_plz_core-v0.22.1) - 2024-05-27

### Other
- update Cargo.toml dependencies

## [0.22.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.21.8...release_plz_core-v0.22.0) - 2024-05-21

### Added
- add package name to changelog context ([#1399](https://github.com/release-plz/release-plz/pull/1399)) ([#1469](https://github.com/release-plz/release-plz/pull/1469))

### Other
- move fixtures directory ([#1467](https://github.com/release-plz/release-plz/pull/1467))

## [0.21.8](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.21.7...release_plz_core-v0.21.8) - 2024-05-14

### Fixed
- don't add directories to github api commit ([#1461](https://github.com/release-plz/release-plz/pull/1461))

## [0.21.7](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.21.6...release_plz_core-v0.21.7) - 2024-05-12

### Fixed
- updating pr persists GitHub verified status ([#1459](https://github.com/release-plz/release-plz/pull/1459))

### Other
- add more error contexts ([#1455](https://github.com/release-plz/release-plz/pull/1455))
- improve error message when updating pr ([#1453](https://github.com/release-plz/release-plz/pull/1453))

## [0.21.6](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.21.5...release_plz_core-v0.21.6) - 2024-05-08

### Added
- print released prs ([#1449](https://github.com/release-plz/release-plz/pull/1449))

### Other
- more descriptive file system error messages ([#1450](https://github.com/release-plz/release-plz/pull/1450))

## [0.21.5](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.21.4...release_plz_core-v0.21.5) - 2024-05-05

### Added
- use cargo registry environment variable to authenticate in private sparse registry ([#1435](https://github.com/release-plz/release-plz/pull/1435))

### Other
- add `needless_pass_by_value` lint ([#1441](https://github.com/release-plz/release-plz/pull/1441))
- add `uninlined_format_args` ([#1440](https://github.com/release-plz/release-plz/pull/1440))
- add clippy lints ([#1439](https://github.com/release-plz/release-plz/pull/1439))
- add `if_not_else` clippy lint ([#1438](https://github.com/release-plz/release-plz/pull/1438))
- update dependencies ([#1437](https://github.com/release-plz/release-plz/pull/1437))

## [0.21.4](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.21.3...release_plz_core-v0.21.4) - 2024-04-28

### Fixed
- json output of release command ([#1421](https://github.com/release-plz/release-plz/pull/1421))

## [0.21.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.21.2...release_plz_core-v0.21.3) - 2024-04-25

### Fixed
- implement `Default` for `Release` struct ([#1418](https://github.com/release-plz/release-plz/pull/1418))

## [0.21.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.21.1...release_plz_core-v0.21.2) - 2024-04-25

### Fixed
- use token to fetch sparse metadata ([#1416](https://github.com/release-plz/release-plz/pull/1416))

## [0.21.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.21.0...release_plz_core-v0.21.1) - 2024-04-21

### Added
- add `release_always` option to release on release PRs only ([#1407](https://github.com/release-plz/release-plz/pull/1407))

### Fixed
- avoid potential deadlock in large repo ([#1408](https://github.com/release-plz/release-plz/pull/1408))

### Other
- fix `if_then_some_else_none` clippy lint ([#1405](https://github.com/release-plz/release-plz/pull/1405))
- don't log cargo stderr ([#1404](https://github.com/release-plz/release-plz/pull/1404))
- improve the github graphql query ([#1383](https://github.com/release-plz/release-plz/pull/1383))

## [0.21.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.20.3...release_plz_core-v0.21.0) - 2024-04-14

### Other
- rename `project-manifest` to `manifest-path` ([#1395](https://github.com/release-plz/release-plz/pull/1395))

## [0.20.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.20.2...release_plz_core-v0.20.3) - 2024-04-08

### Added
- add json output ([#1386](https://github.com/release-plz/release-plz/pull/1386)) ([#1368](https://github.com/release-plz/release-plz/pull/1368))

### Other
- document `get_registry_packages` function ([#1381](https://github.com/release-plz/release-plz/pull/1381))

## [0.20.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.20.1...release_plz_core-v0.20.2) - 2024-04-02

### Fixed
- initialize git repo for downloaded packages ([#1377](https://github.com/release-plz/release-plz/pull/1377))

### Other
- update dependencies ([#1379](https://github.com/release-plz/release-plz/pull/1379))
- *(deps)* bump git-cliff-core to v2 ([#1361](https://github.com/release-plz/release-plz/pull/1361))
- emit a warning on readme file mismatch ([#1363](https://github.com/release-plz/release-plz/pull/1363))

## [0.20.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.20.0...release_plz_core-v0.20.1) - 2024-03-24

### Other
- update dependencies ([#1358](https://github.com/release-plz/release-plz/pull/1358))

## [0.20.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.19.8...release_plz_core-v0.20.0) - 2024-03-14

### Added
- allow custom `git_release_body` ([#1341](https://github.com/release-plz/release-plz/pull/1341))

## [0.19.8](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.19.7...release_plz_core-v0.19.8) - 2024-03-10

### Fixed
- allow rust project in subdir ([#1336](https://github.com/release-plz/release-plz/pull/1336))

### Other
- use `camino` ([#1337](https://github.com/release-plz/release-plz/pull/1337))

## [0.19.7](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.19.6...release_plz_core-v0.19.7) - 2024-03-10

### Added
- create annotated tags instead of lightweight ([#1255](https://github.com/release-plz/release-plz/pull/1255))

## [0.19.6](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.19.5...release_plz_core-v0.19.6) - 2024-03-08

### Fixed
- check diffs in Cargo.toml custom README path ([#1315](https://github.com/release-plz/release-plz/pull/1315))

### Other
- use edition and license workspace ([#1329](https://github.com/release-plz/release-plz/pull/1329))

## [0.19.5](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.19.4...release_plz_core-v0.19.5) - 2024-03-06

### Added
- Add `publish_features` to pass feature list to `cargo publish` ([#1320](https://github.com/release-plz/release-plz/pull/1320))

## [0.19.4](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.19.3...release_plz_core-v0.19.4) - 2024-02-27

### Other
- error if committed Cargo.lock is in `.gitignore` ([#1294](https://github.com/release-plz/release-plz/pull/1294))

## [0.19.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.19.2...release_plz_core-v0.19.3) - 2024-02-25

### Other
- don't log big release request ([#1300](https://github.com/release-plz/release-plz/pull/1300))

## [0.19.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.19.1...release_plz_core-v0.19.2) - 2024-02-25

### Added
- prepare release if commits respect the `release_commits` regex ([#1278](https://github.com/release-plz/release-plz/pull/1278))

### Other
- update cargo to v0.77 ([#1296](https://github.com/release-plz/release-plz/pull/1296))

## [0.19.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.19.0...release_plz_core-v0.19.1) - 2024-02-23

### Added
- add `all-static` feature ([#1287](https://github.com/release-plz/release-plz/pull/1287))

### Fixed
- allow configuring the `release` flag ([#1290](https://github.com/release-plz/release-plz/pull/1290))

## [0.19.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.18.0...release_plz_core-v0.19.0) - 2024-02-11

### Added
- allow customizing git release name with tera template. [#677](https://github.com/release-plz/release-plz/pull/677) ([#1260](https://github.com/release-plz/release-plz/pull/1260))

## [0.18.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.17.1...release_plz_core-v0.18.0) - 2024-02-09

### Added
- allow customizing git tag name with tera template ([#1256](https://github.com/release-plz/release-plz/pull/1256))

## [0.17.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.17.0...release_plz_core-v0.17.1) - 2024-02-06

### Fixed
- apply default changelog git config ([#1243](https://github.com/release-plz/release-plz/pull/1243))
- update local dependencies specified in the workspace manifest ([#1251](https://github.com/release-plz/release-plz/pull/1251))
- check cargo token only when publishing ([#1250](https://github.com/release-plz/release-plz/pull/1250))

### Other
- fix tests on mac ([#1242](https://github.com/release-plz/release-plz/pull/1242))

## [0.17.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.16.2...release_plz_core-v0.17.0) - 2024-01-26

### Added
- add `git_release_type` configuration option to allow GitHub/Gitea prereleases ([#1228](https://github.com/release-plz/release-plz/pull/1228))

### Fixed
- support rust-toolchain.toml file ([#1234](https://github.com/release-plz/release-plz/pull/1234))

### Other
- add context to some errors ([#1232](https://github.com/release-plz/release-plz/pull/1232))

## [0.16.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.16.1...release_plz_core-v0.16.2) - 2024-01-23

### Added
- use github graphql api for commits to have the GitHub "Verified" badge on release-plz commits
  ([#1201](https://github.com/release-plz/release-plz/pull/1201))

## [0.16.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.16.0...release_plz_core-v0.16.1) - 2024-01-20

### Fixed
- Correct dependency update behavior for release-pr ([#1217](https://github.com/release-plz/release-plz/pull/1217))

### Other
- update dependencies ([#1213](https://github.com/release-plz/release-plz/pull/1213))

## [0.16.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.15.5...release_plz_core-v0.16.0) - 2024-01-16

### Added
- Add release flag ([#1098](https://github.com/release-plz/release-plz/pull/1098))

### Fixed
- Prevent error if Cargo.lock doesn't exist during portions of commit history ([#1205](https://github.com/release-plz/release-plz/pull/1205))

### Other
- improve public packages error message ([#1187](https://github.com/release-plz/release-plz/pull/1187))
- add debug statement when Cargo.toml differs ([#1184](https://github.com/release-plz/release-plz/pull/1184))
- less verbose logs ([#1183](https://github.com/release-plz/release-plz/pull/1183))

## [0.15.5](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.15.4...release_plz_core-v0.15.5) - 2023-12-30

### Other
- remove unuseful function ([#1166](https://github.com/release-plz/release-plz/pull/1166))
- simplify code ([#1165](https://github.com/release-plz/release-plz/pull/1165))

## [0.15.4](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.15.3...release_plz_core-v0.15.4) - 2023-12-19

### Fixed
- restore changes introduced by `cargo package` ([#1152](https://github.com/release-plz/release-plz/pull/1152))

## [0.15.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.15.2...release_plz_core-v0.15.3) - 2023-12-16

### Added
- return error if tag exists and package isn't published ([#1143](https://github.com/release-plz/release-plz/pull/1143))
- support packages with git dependencies ([#1141](https://github.com/release-plz/release-plz/pull/1141))
- distinguish dependency update type ([#1140](https://github.com/release-plz/release-plz/pull/1140))

### Fixed
- internal dependency conflict ([#1135](https://github.com/release-plz/release-plz/pull/1135))

### Other
- *(deps)* bump cargo to 0.75.1 ([#1137](https://github.com/release-plz/release-plz/pull/1137))

## [0.15.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.15.1...release_plz_core-v0.15.2) - 2023-12-13

### Fixed
- pass previous version to git-cliff ([#1134](https://github.com/release-plz/release-plz/pull/1134))

## [0.15.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.15.0...release_plz_core-v0.15.1) - 2023-12-04

### Fixed
- support projects with external readme ([#1110](https://github.com/release-plz/release-plz/pull/1110))
- pass full commit message to git-cliff ([#1103](https://github.com/release-plz/release-plz/pull/1103)) ([#1104](https://github.com/release-plz/release-plz/pull/1104))

## [0.15.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.14.5...release_plz_core-v0.15.0) - 2023-11-30

### Added
- add publish_timeout to avoid release blocking issue, fix [#1015](https://github.com/release-plz/release-plz/pull/1015) ([#1088](https://github.com/release-plz/release-plz/pull/1088))
- prevent typos in overrides ([#1080](https://github.com/release-plz/release-plz/pull/1080))
- Update a package only if edited file belongs to `cargo package --list` ([#1089](https://github.com/release-plz/release-plz/pull/1089))

### Fixed
- resolve issue on Windows machines that use CRLF that would duplicate the header on each update ([#1083](https://github.com/release-plz/release-plz/pull/1083))

### Other
- document Gitea releases ([#1076](https://github.com/release-plz/release-plz/pull/1076))

## [0.14.5](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.14.4...release_plz_core-v0.14.5) - 2023-11-08

### Added
- add `pr_draft` config option ([#1061](https://github.com/release-plz/release-plz/pull/1061))

## [0.14.4](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.14.3...release_plz_core-v0.14.4) - 2023-10-27

### Fixed
- use registry argument on publish ([#1050](https://github.com/release-plz/release-plz/pull/1050))
- use `release-plz-` rather than `release-plz/` branch prefix ([#1041](https://github.com/release-plz/release-plz/pull/1041))

## [0.14.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.14.2...release_plz_core-v0.14.3) - 2023-10-15

### Fixed
- ignore `.ignore` files ([#1036](https://github.com/release-plz/release-plz/pull/1036))

## [0.14.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.14.1...release_plz_core-v0.14.2) - 2023-09-30

### Other
- update dependencies

## [0.14.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.14.0...release_plz_core-v0.14.1) - 2023-09-30

### Added
- avoid copying gitignored files ([#1000](https://github.com/release-plz/release-plz/pull/1000)) ([#1001](https://github.com/release-plz/release-plz/pull/1001))

### Fixed
- parse changelog header correctly ([#1007](https://github.com/release-plz/release-plz/pull/1007))

## [0.14.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.13.6...release_plz_core-v0.14.0) - 2023-09-24

### Added
- add ability to create draft git release ([#986](https://github.com/release-plz/release-plz/pull/986))

### Fixed
- respect git-cliff sort order ([#985](https://github.com/release-plz/release-plz/pull/985))

## [0.13.6](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.13.5...release_plz_core-v0.13.6) - 2023-09-17

### Fixed
- don't publish examples ([#974](https://github.com/release-plz/release-plz/pull/974))
- don't discard error context ([#971](https://github.com/release-plz/release-plz/pull/971))

## [0.13.5](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.13.4...release_plz_core-v0.13.5) - 2023-09-16

### Added
- allow disabling git tag ([#968](https://github.com/release-plz/release-plz/pull/968))
- pass commit ids to git-cliff ([#967](https://github.com/release-plz/release-plz/pull/967))

### Other
- add additional clippy lints ([#965](https://github.com/release-plz/release-plz/pull/965))

## [0.13.4](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.13.3...release_plz_core-v0.13.4) - 2023-09-11

### Added
- *(release-pr)* sign release-plz commit ([#956](https://github.com/release-plz/release-plz/pull/956))

## [0.13.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.13.2...release_plz_core-v0.13.3) - 2023-09-08

### Other
- update dependencies ([#949](https://github.com/release-plz/release-plz/pull/949))
- *(ci)* check links ([#941](https://github.com/release-plz/release-plz/pull/941))
- fix clippy lint ([#931](https://github.com/release-plz/release-plz/pull/931))

## [0.13.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.13.1...release_plz_core-v0.13.2) - 2023-08-22

### Other
- static openssl ([#920](https://github.com/release-plz/release-plz/pull/920))
- improve http error messages ([#921](https://github.com/release-plz/release-plz/pull/921))
- update git-cliff ([#919](https://github.com/release-plz/release-plz/pull/919))
- test release-plz release ([#892](https://github.com/release-plz/release-plz/pull/892))

## [0.13.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.13.0...release_plz_core-v0.13.1) - 2023-08-16

### Fixed
- release in https git repos ([#912](https://github.com/release-plz/release-plz/pull/912))

## [0.13.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.12.4...release_plz_core-v0.13.0) - 2023-08-14

### Added
- add `changelog_include` option ([#904](https://github.com/release-plz/release-plz/pull/904))

### Other
- add tests for gitea ([#421](https://github.com/release-plz/release-plz/pull/421))

## [0.12.4](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.12.3...release_plz_core-v0.12.4) - 2023-08-02

### Fixed
- update workspace version in dependencies ([#889](https://github.com/release-plz/release-plz/pull/889))

## [0.12.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.12.2...release_plz_core-v0.12.3) - 2023-07-25

### Added
- *(release)* add support for sparse registry URLs ([#863](https://github.com/release-plz/release-plz/pull/863))

## [0.12.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.12.1...release_plz_core-v0.12.2) - 2023-06-26

### Fixed
- copy symlinks ([#827](https://github.com/release-plz/release-plz/pull/827))

### Other
- small package comparison refactor ([#833](https://github.com/release-plz/release-plz/pull/833))

## [0.12.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.12.0...release_plz_core-v0.12.1) - 2023-06-10

### Fixed
- ignore Cargo.lock dev dependencies changes ([#820](https://github.com/release-plz/release-plz/pull/820))

## [0.12.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.11.3...release_plz_core-v0.12.0) - 2023-06-09

### Fixed
- update changelog correctly when workspace version specified ([#816](https://github.com/release-plz/release-plz/pull/816))

## [0.11.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.11.2...release_plz_core-v0.11.3) - 2023-06-09

### Other
- update dependencies ([#814](https://github.com/release-plz/release-plz/pull/814))
- stop looking at git history if commit tagged ([#813](https://github.com/release-plz/release-plz/pull/813))

## [0.11.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.11.1...release_plz_core-v0.11.2) - 2023-05-31

### Fixed
- *(workspaces)* report correct version update ([#802](https://github.com/release-plz/release-plz/pull/802))

## [0.11.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.11.0...release_plz_core-v0.11.1) - 2023-05-24

### Added
- add pr/issue link to changelog entries (#793)

### Other
- parse cargo lock faster (#795)

## [0.11.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.10.0...release_plz_core-v0.11.0) - 2023-05-21

### Added
- check if `Cargo.lock` packages were updated (#784)

### Fixed
- support nested crates (#783)

## [0.10.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.9.6...release_plz_core-v0.10.0) - 2023-05-08

### Other
- run cargo-semver-check in parallel (#766)
- represent semver_check config as bool (#765)

## [0.9.6](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.9.5...release_plz_core-v0.9.6) - 2023-05-07

### Other
- Performance improvement: run semver-checks only on changed packages (#754)

## [0.9.5](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.9.4...release_plz_core-v0.9.5) - 2023-05-07

### Fixed
- abort failed rebase (#760)

## [0.9.4](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.9.3...release_plz_core-v0.9.4) - 2023-05-05

### Added
- Add `publish` config option to disable publishing to the cargo registry (#718)

### Other
- *(refactor)* move git files under `git` module (#753)

## [0.9.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.9.2...release_plz_core-v0.9.3) - 2023-04-27

### Fixed
- don't compare ignored files (#739)

## [0.9.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.9.1...release_plz_core-v0.9.2) - 2023-04-25

### Fixed
- downgrade cargo to fix windows compilation

## [0.9.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.9.0...release_plz_core-v0.9.1) - 2023-04-24

### Fixed
- only add commit title in changelog (#729)

## [0.9.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.8.0...release_plz_core-v0.9.0) - 2023-04-21

### Added
- add `pr_labels` configuration option to add labels to the PR opened by release-plz (#708)

## [0.8.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.7.1...release_plz_core-v0.8.0) - 2023-04-16

### Added
- [**breaking**] changed config file format. See [docs](https://release-plz.dev/docs/config.html).
- [**breaking**] removed `--git-release` flag. Now git releases are enabled by default.
  You can disable them with the `git_release_enable` configuration option.
- make cargo publish flags configurable (#684)

### Fixed
- config package override (#695)
- don't return early when publishing crates (#691)

## [0.7.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.7.0...release_plz_core-v0.7.1) - 2023-04-05

### Fixed
- changelog path handling (#669)
- detect allow-dirty error (#666)

## [0.7.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.6.2...release_plz_core-v0.7.0) - 2023-04-02

### Added
- allow to provide a custom changelog path (#653)

## [0.6.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.6.1...release_plz_core-v0.6.2) - 2023-04-02

### Added
- detect custom changelog header (#651)

## [0.6.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.6.0...release_plz_core-v0.6.1) - 2023-04-02

### Fixed
- read opened PRs with empty body (#649)

## [0.6.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.5.8...release_plz_core-v0.6.0) - 2023-04-01

### Added
- Add config file. See the [docs](https://release-plz.dev/docs/config.html) (#634)

## [0.5.8](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.5.7...release_plz_core-v0.5.8) - 2023-03-27

### Added
- Support the Cargo.toml field `workspace.package.version` (#638).

## [0.5.7](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.5.6...release_plz_core-v0.5.7) - 2023-03-19

### Added
- *(release)* add GitLab support (#591).
  `release-plz release-pr` GitLab support is still missing.

## [0.5.6](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.5.5...release_plz_core-v0.5.6) - 2023-03-17

### Added
- detect git remote (#610)

### Fixed
- update pr: do git fetch before rebase (#607)

## [0.5.5](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.5.4...release_plz_core-v0.5.5) - 2023-03-13

### Added
- write changelog in pr body (#598)

## [0.5.4](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.5.3...release_plz_core-v0.5.4) - 2023-03-10

### Fix
- update to cargo v0.69 to be compatible with sparse index.

## [0.5.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.5.2...release_plz_core-v0.5.3) - 2023-03-09

### Added
- include version in pr title for single crate (#593)

### Other
- retry failing http calls (#585)

## [0.5.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.5.1...release_plz_core-v0.5.2) - 2023-03-04

### Added
- detect circular dependency (#581)

## [0.5.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.5.0...release_plz_core-v0.5.1) - 2023-02-27

### Fixed
- remove ansi escape sequences in cargo-semver-checks output (#575)

## [0.5.0](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.21...release_plz_core-v0.5.0) - 2023-02-26

### Added
- Add cargo-semver-checks integration. If the `cargo-semver-checks` binary is present, release-plz uses
  it to check semver compatibility. If `cargo-semver-checks` detects an API breaking change, release-plz
  updates the major version. (#568)

### Fixed
- when editing a release-pr, update pr title and body (#571)

## [0.4.21](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.20...release_plz_core-v0.4.21) - 2023-02-20

### Other
- remove unused check (#559)

## [0.4.20](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.19...release_plz_core-v0.4.20) - 2023-02-18

### Fixed
- *(release)* trust gh workspace (#553)

## [0.4.19](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.18...release_plz_core-v0.4.19) - 2023-02-18

### Other
- `release-plz release` creates a release in Gitea, too (#539)

## [0.4.18](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.17...release_plz_core-v0.4.18) - 2023-02-11

### Added
- *(release)* add the possibility to add `--no-verify` and `--allow-dirty` as cargo publish flags (#532)

## [0.4.17](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.16...release_plz_core-v0.4.17) - 2023-02-10

### Added
- update pr in gitea (#530)

### Fixed
- update branch from main before updating PR (#528)

## [0.4.16](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.15...release_plz_core-v0.4.16) - 2023-02-08

### Added
- add changelog changes to gitea (#525)
- log published version (#514)

## [0.4.15](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.14...release_plz_core-v0.4.15) - 2023-01-31

### Fixed
- *(release)* trust github workspace dir (#512)

## [0.4.14](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.13...release_plz_core-v0.4.14) - 2023-01-31

### Fixed
- handle new crate correctly (#509, #511)

### Other
- improve log (#502)

## [0.4.13](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.12...release_plz_core-v0.4.13) - 2023-01-27

### Fixed
- fix edit pr when a new file is present (#498)

### Other
- improve logging (#500)

## [0.4.12](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.11...release_plz_core-v0.4.12) - 2023-01-26

### Added
- edit GitHub release pr instead of closing it (#470)

### Other
- fix cargo clippy (#489)

## [0.4.11](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.10...release_plz_core-v0.4.11) - 2023-01-22

### Other
- update cargo (#473)

## [0.4.10](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.9...release_plz_core-v0.4.10) - 2023-01-17

### Other
- remove `octocrab` dependency (#467)

## [0.4.9](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.8...release_plz_core-v0.4.9) - 2023-01-16

### Added
- *(release-pr)* do not include the crate name if no workspace (#461)

### Fixed
- wrong log line (#464)

### Other
- fix typo in code (#463)

## [0.4.8](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.7...release_plz_core-v0.4.8) - 2023-01-16

### Fixed
- do not update changelog if new version exists (#452)
- changelog: fix link to first change (#450)

## [0.4.7](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.6...release_plz_core-v0.4.7) - 2023-01-15

### Added
- do not prefix crate name in tag for single crate projects (#444)

## [0.4.6](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.5...release_plz_core-v0.4.6) - 2023-01-12

### Added
- Include previous version in Pr Body (#430)

## [0.4.5](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.4...release_plz_core-v0.4.5) - 2023-01-11

### Other
- don't remove build metadata (#433)
- handle pre-releases (#425)

## [0.4.4](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.3...release_plz_core-v0.4.4) - 2023-01-07

### Added
- add body to git release (#411)

### Fixed
- *(release)* git-token is optional (#413)

## [0.4.3](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.2...release_plz_core-v0.4.3) - 2023-01-06

### Added
- Initial experimental support for GitHub releases.

### Other
- print error kind when copying directories (#408)
- use secret strings for tokens (#403)

## [0.4.2](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.1...release_plz_core-v0.4.2) - 2022-12-26

### Other
- fix additional clippy lints (#379)

## [0.4.1](https://github.com/release-plz/release-plz/compare/release_plz_core-v0.4.0...release_plz_core-v0.4.1) - 2022-12-16

### Other
- remove `cargo-edit` dependency (#375)
- Add support for Gitea repositories (#372)

## [0.4.0] - 2022-12-12

### Added
- [**breaking**] Changelog: add GitHub release link to show the commits since the previous version (#371)

## [0.3.1] - 2022-11-17

### Changed
- don't release if the tag exists (#342)

## [0.3.0] - 2022-11-12

### Fixed
- [**breaking**] remove deprecated chrono::Date (#340)

## [0.2.21] - 2022-11-04

### Fixed
- clippy lints (#332)

### Other
- use workspace dependencies (#333)

## [0.2.20] - 2022-11-03

### Fixed
- *(typo)* update code comment for `Diff` struct field (#329)

## [0.2.19] - 2022-10-24

### Fixed
- include function removed from cargo-edit (#305)

### Other
- bump dependencies
- use cargo-clone-core library (#302)
- fix clippy lint (#297)

## [0.2.18] - 2022-07-16

### Fixed
- support symlinks (#213)

### Other
- update git cliff to version 8 (#212)
- update to cargo edit 10 (#211)
- *(deps)* bump serde_json from 1.0.81 to 1.0.82 (#202)
- remove unused lifetime (#206)

## [0.2.17] - 2022-06-18

### Other
- update clap and fake libraries (#186)
- *(deps)* bump tracing from 0.1.34 to 0.1.35 (#179)
- *(deps)* bump tokio from 1.19.1 to 1.19.2 (#178)
- *(deps)* bump tokio from 1.18.2 to 1.19.1 (#175)

## [0.2.16] - 2022-05-29

### Added
- add --allow-dirty flag to update command (#169)

## [0.2.15] - 2022-05-28

### Other
- skip pr field in instrument (#165)

## [0.2.14] - 2022-05-28

### Fixed
- remove link parsers (#155)

### Other
- *(deps)* bump crates-index from 0.18.7 to 0.18.8 (#144)
- *(deps)* bump expect-test from 1.2.2 to 1.3.0 (#141)

## [0.2.13] - 2022-05-26

### Added
- improve PR body (#139)

## [0.2.12] - 2022-05-19

### Other
- upgrade dependencies (#133)

## [0.2.11] - 2022-05-14

### Other
- updated the following local packages: git_cmd

## [0.2.10] - 2022-05-13

### Added
- read custom git cliff config (#126)

## [0.2.9] - 2022-05-10

### Added
- add ability to update all the dependencies in the lockfile (#123)

## [0.2.8] - 2022-05-08

### Added
- update package if one of its local dependencies is updated (#112)

## [0.2.7] - 2022-05-02

### Fixed
- *(release)* do not discard error (#105)

## [0.2.6] - 2022-05-01

### Fixed
- fix cargo update (#99)

### Changed
- update local dependencies versions (#102)
- remove default features from some deps (#101)

## [0.2.5] - 2022-04-27

### Added
- add `release` command (#89)
- Publish tag (#92)

### Fixed
- fix package equality

### Other
- update dependencies (#91)

## [0.2.4] - 2022-04-23

### Added
- *(release-pr)* close old release-plz prs when running release-plz (#81)
- update Cargo.lock, too (#78)

## [0.2.3] - 2022-04-11

### Fixed
- generate changelog for new packages (#72)
- add support for unpublished packages (#71)

### Changed
- name new branch as timestamp (#70)

## [0.2.2] - 2022-04-10

### Fixed
- remove all unwraps that are not in tests (#49)

### Other
- write package name and version in PR name (#58)
- improve workspace members error (#56)

## [0.2.1] - 2022-03-30

### Fixed
- fix changelog for release-pr (#39)
- remove changelog unwrap (#37)

### Added
- support alternative registries (#34)

### Other
- update crate description

## [0.2.0] - 2022-03-27

### Added
- [**breaking**] generate changelog with git-cliff (#29)

### Fixed
- fix breaking change message
- fix repository link
