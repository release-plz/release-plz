# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.130](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.129...release-plz-v0.3.130) - 2025-04-02

### Fixed

- ignore global `.gitignore` when copying to tmp dir ([#2149](https://github.com/release-plz/release-plz/pull/2149))

### Other

- update Cargo.lock dependencies

## [0.3.129](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.128...release-plz-v0.3.129) - 2025-03-26

### Fixed

- update a package if dependency of its dependency was updated ([#2150](https://github.com/release-plz/release-plz/pull/2150))

## [0.3.128](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.127...release-plz-v0.3.128) - 2025-03-22

### Other

- *(init)* explain how to create the PAT ([#2141](https://github.com/release-plz/release-plz/pull/2141))

## [0.3.127](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.126...release-plz-v0.3.127) - 2025-03-17

### Fixed

- escape the curly braces in the template for init ([#2138](https://github.com/release-plz/release-plz/pull/2138))

### Other

- add more rust lints ([#2135](https://github.com/release-plz/release-plz/pull/2135))
- explain why we use the `crates_index` crate ([#2134](https://github.com/release-plz/release-plz/pull/2134))

## [0.3.126](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.125...release-plz-v0.3.126) - 2025-03-15

### Fixed

- deduplicate contributors ([#2110](https://github.com/release-plz/release-plz/pull/2110))

### Other

- don't run action in forks ([#2128](https://github.com/release-plz/release-plz/pull/2128))
- remove verbose cargo logs ([#2129](https://github.com/release-plz/release-plz/pull/2129))

## [0.3.125](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.124...release-plz-v0.3.125) - 2025-03-04

### Other

- document GitLab support ([#2109](https://github.com/release-plz/release-plz/pull/2109))
- use git-cliff to expand the PR link ([#2103](https://github.com/release-plz/release-plz/pull/2103))

## [0.3.124](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.123...release-plz-v0.3.124) - 2025-03-01

### Fixed

- restore pr link expansion on non-conventional commits (#2102)

### Other

- test raw_message feature ([#2101](https://github.com/release-plz/release-plz/pull/2101))
- add new line to changelog header (#2098)

## [0.3.123](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.122...release-plz-v0.3.123) - 2025-02-25

### Added

- edit changelog order to respect release order (#2070)

### Other

- split next_ver module (#2095)
- simplify logging setup (#2071)

## [0.3.122](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.121...release-plz-v0.3.122) - 2025-02-22

### Fixed

- don't error if package already published (#2085)

### Other

- use edition 2024 (#2086)
- enable more clippy lints (#1971)

## [0.3.121](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.120...release-plz-v0.3.121) - 2025-02-22

### Added

- support rust 2024 edition (#2084)

### Fixed

- clippy lint (#2082)
- support env var override for registry index (#2033)

### Other

- test that binary is updated if library changes ([#2069](https://github.com/release-plz/release-plz/pull/2069))
- Run some tests in CI that were ignored (#2065)
- add style to the command line (#2059)

## [0.3.120](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.119...release-plz-v0.3.120) - 2025-02-09

### Fixed

- Bring back breaking change description in pr body (#2055)
- Show previous version in PR body (#2054)
- Check semver breaking changes on packages containing both library and binary (#2053)
- Show in pr body when semver check passed ([#2057](https://github.com/release-plz/release-plz/pull/2057))

### Other

- hint to install cargo semver checks to run tests (#2056)
- remove duration-str dependency (#2047)
- remove direct dipendency on lazy_static (#2049)

## [0.3.119](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.118...release-plz-v0.3.119) - 2025-02-09

### Fixed

- Don't fail if you can't fetch associated PRs from the remote because the commit hasn't been pushed (#2045)

## [0.3.118](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.117...release-plz-v0.3.118) - 2025-02-08

### Fixed

- correctly detect if package is executable (#2041)

## [0.3.117](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.116...release-plz-v0.3.117) - 2025-02-08

### Added

- add releases to release-pr json output (#2031)

### Fixed

- don't fail if commit wasn't pushed to remote (#2037)
- correctly detect if package contains executable (#2024)

## [0.3.116](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.115...release-plz-v0.3.116) - 2025-02-01

### Fixed

- don't update libraries versions on Cargo.lock update (#2016)

## [0.3.115](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.114...release-plz-v0.3.115) - 2025-02-01

### Added

- Download packages from different registries in parallel to improve performance (#2005)

### Fixed

- Don't try to fetch information from the remote if it's a "filler" commit (#2014)

### Other

- ♻️ refactor: use tera template for default PR body ([#1861](https://github.com/release-plz/release-plz/pull/1861))

## [0.3.114](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.113...release-plz-v0.3.114) - 2025-01-23

### Fixed

- don't suppress git error when determining commits to release (#1962)
- *(core)* Stash uncommited changes in dirty repos during update (#1982)
- *(core)* Make `release --dry-run` more informative (#1983)

### Other

- update Cargo.lock dependencies
- allow fetching latest package summaries from cargo registry ([#1984](https://github.com/release-plz/release-plz/pull/1984))

## [0.3.113](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.112...release-plz-v0.3.113) - 2025-01-14

### Added

- allow adding labels to Gitea release PR (#1937)

### Fixed

- Make release-pr tests use UTC rather than local timezone (#1977)
- Use dunce to canonicalize package paths correctly (#1975)
- Fix `release-plz` init local dependency check (#1978)

### Other

- refactor to remove duplicate string (#1964)
- move permissions to workflow jobs (#1930)

## [0.3.112](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.111...release-plz-v0.3.112) - 2024-12-19

### Fixed

- increment pre-release version correctly when dependencies change (#1915)
- Fixed wrong link on github token usage for release ci. (#1913)

### Other

- log when a release is skipped because of release_commits regex (#1904)

## [0.3.111](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.110...release-plz-v0.3.111) - 2024-12-07

### Added

- warn if cargo-semver-checks is not installed ([#1891](https://github.com/release-plz/release-plz/pull/1891))
- Enable `pr_labels` for GitLab backend ([#1879](https://github.com/release-plz/release-plz/pull/1879))
- add raw_message to commit context ([#1867](https://github.com/release-plz/release-plz/pull/1867))

### Fixed

- locate Cargo.lock correctly when project isn't in root directory ([#1894](https://github.com/release-plz/release-plz/pull/1894))

### Other

- Make logging less verbose ([#1840](https://github.com/release-plz/release-plz/pull/1840))
- Fix elided lifetime issue ([#1875](https://github.com/release-plz/release-plz/pull/1875))
- test default release pr configuration ([#1862](https://github.com/release-plz/release-plz/pull/1862))

## [0.3.110](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.109...release-plz-v0.3.110) - 2024-11-21

### Added

- *(init)* check if Cargo.toml constains the `version` field in local dependencies ([#1843](https://github.com/release-plz/release-plz/pull/1843))

## [0.3.109](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.108...release-plz-v0.3.109) - 2024-11-21

### Fixed

- don't update changelog if version is already present ([#1834](https://github.com/release-plz/release-plz/pull/1834))

### Other

- move `MarcoIeni/release-plz` to `release-plz/release-plz` ([#1850](https://github.com/release-plz/release-plz/pull/1850))
- move `MarcoIeni/release-plz-action` to `release-plz/action` ([#1849](https://github.com/release-plz/release-plz/pull/1849))
- migrate website to release-plz.dev ([#1842](https://github.com/release-plz/release-plz/pull/1842))

## [0.3.108](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.107...release-plz-v0.3.108) - 2024-11-11

### Added

- *(init)*: add check for mandatory fields in Cargo.toml ([#1769](https://github.com/release-plz/release-plz/pull/1769))

### Other

- remove async_trait ([#1824](https://github.com/release-plz/release-plz/pull/1824))

## [0.3.107](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.106...release-plz-v0.3.107) - 2024-11-08

### Added

- add `publish_all_features` config option to workspace/package config ([#1818](https://github.com/release-plz/release-plz/pull/1818))

## [0.3.106](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.105...release-plz-v0.3.106) - 2024-10-30

### Fixed

- Correctly deserialize GitLab MR commits ([#1808](https://github.com/release-plz/release-plz/pull/1808))

### Other

- update Cargo.toml dependencies

## [0.3.105](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.104...release-plz-v0.3.105) - 2024-10-27

### Fixed

- revert Cargo.lock changes after running `cargo package` ([#1803](https://github.com/release-plz/release-plz/pull/1803))

## [0.3.104](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.103...release-plz-v0.3.104) - 2024-10-25

### Fixed

- typo in CLI log level help ([#1796](https://github.com/release-plz/release-plz/pull/1796))
- release squashed release PRs from main branch ([#1801](https://github.com/release-plz/release-plz/pull/1801))

## [0.3.103](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.102...release-plz-v0.3.103) - 2024-10-18

### Added

- add pr_body field ([#1778](https://github.com/release-plz/release-plz/pull/1778))
- add breaking_changes field to pr_body context ([#1787](https://github.com/release-plz/release-plz/pull/1787))

### Other

- improve release-pr error message if token missing ([#1784](https://github.com/release-plz/release-plz/pull/1784))

## [0.3.102](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.101...release-plz-v0.3.102) - 2024-10-16

### Fixed

- *(action)* clone entire history for release ([#1777](https://github.com/release-plz/release-plz/pull/1777))
- fix release for squash commits ([#1775](https://github.com/release-plz/release-plz/pull/1775))

## [0.3.101](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.100...release-plz-v0.3.101) - 2024-10-16

### Fixed

- fix github token for the init command ([#1764](https://github.com/release-plz/release-plz/pull/1764))

## [0.3.100](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.99...release-plz-v0.3.100) - 2024-10-15

### Fixed

- checkout on release commit ([#1761](https://github.com/release-plz/release-plz/pull/1761))

## [0.3.99](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.98...release-plz-v0.3.99) - 2024-10-13

### Added

- add `pr_name` config field ([#1765](https://github.com/release-plz/release-plz/pull/1765))
- add `remote.pr_number` to `remote` context of commits in changelog ([#1768](https://github.com/release-plz/release-plz/pull/1768))

### Other

- suggest running commands in parallel in action ([#1758](https://github.com/release-plz/release-plz/pull/1758))

## [0.3.98](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.97...release-plz-v0.3.98) - 2024-10-08

### Added

- add `pr_branch_prefix` option to configure branch prefix for release-pr ([#1728](https://github.com/release-plz/release-plz/pull/1728)) ([#1737](https://github.com/release-plz/release-plz/pull/1737))

## [0.3.97](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.96...release-plz-v0.3.97) - 2024-10-05

### Other

- *(github action)* suggest using `concurrency` ([#1744](https://github.com/release-plz/release-plz/pull/1744))

## [0.3.96](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.95...release-plz-v0.3.96) - 2024-10-05

### Added

- add contributors to git release body context ([#1736](https://github.com/release-plz/release-plz/pull/1736))

### Other

- fix `unnecessary_lazy_evaluations`lint ([#1734](https://github.com/release-plz/release-plz/pull/1734))
- fix `too_many_arguments` lint ([#1733](https://github.com/release-plz/release-plz/pull/1733))

## [0.3.95](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.94...release-plz-v0.3.95) - 2024-10-03

### Added

- add contributors' username to changelog context ([#1731](https://github.com/release-plz/release-plz/pull/1731))
- add remote contributor username to commit context of changelog ([#1635](https://github.com/release-plz/release-plz/pull/1635))

## [0.3.94](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.93...release-plz-v0.3.94) - 2024-10-01

### Fixed

- changelogs only include commits that edit packaged files ([#1703](https://github.com/release-plz/release-plz/pull/1703))

### Other

- update Cargo.lock dependencies

## [0.3.93](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.92...release-plz-v0.3.93) - 2024-09-30

### Fixed

- retrieve token for each package's registry ala `cargo:token` ([#1669](https://github.com/release-plz/release-plz/pull/1669))
- Disable changelog in PR body if it goes over github character limit ([#1716](https://github.com/release-plz/release-plz/pull/1716))

## [0.3.92](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.91...release-plz-v0.3.92) - 2024-09-28

### Added

- Add `features_always_increment_minor` flag ([#1657](https://github.com/release-plz/release-plz/pull/1657))

### Other

- update to secrecy 10 ([#1719](https://github.com/release-plz/release-plz/pull/1719))
- update to git-cliff 2.6 ([#1718](https://github.com/release-plz/release-plz/pull/1718))

## [0.3.91](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.90...release-plz-v0.3.91) - 2024-09-17

### Added

- support local dependencies in workspace manifest ([#1695](https://github.com/release-plz/release-plz/pull/1695))

### Other

- update Cargo.lock dependencies
- improve `dependencies_to_update` fn ([#1694](https://github.com/release-plz/release-plz/pull/1694))
- add comments to package update code ([#1692](https://github.com/release-plz/release-plz/pull/1692))
- extract function from `packages_to_update` ([#1691](https://github.com/release-plz/release-plz/pull/1691))
- improve `dependent_packages` function ([#1689](https://github.com/release-plz/release-plz/pull/1689))
- improve dependencies_to_update function ([#1687](https://github.com/release-plz/release-plz/pull/1687))

## [0.3.90](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.89...release-plz-v0.3.90) - 2024-09-16

### Fixed

- restore semver check report ([#1684](https://github.com/release-plz/release-plz/pull/1684))

## [0.3.89](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.88...release-plz-v0.3.89) - 2024-09-16

### Added

- allow specifying `version_group` field in package config ([#1661](https://github.com/release-plz/release-plz/pull/1661))

### Other

- improve invalid file error message ([#1682](https://github.com/release-plz/release-plz/pull/1682))
- Make the banner image lead people straight to the website ([#1679](https://github.com/release-plz/release-plz/pull/1679))

## [0.3.88](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.87...release-plz-v0.3.88) - 2024-09-14

### Fixed

- fallback to http 1.1 when http2 is not supported on fetching sparse metadata ([#1676](https://github.com/release-plz/release-plz/pull/1676))

## [0.3.87](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.86...release-plz-v0.3.87) - 2024-09-07

### Fixed
- align changelog line breaks with keepachangelog spec ([#1653](https://github.com/release-plz/release-plz/pull/1653))
- verify correctly if Cargo.lock is ignored and committed ([#1662](https://github.com/release-plz/release-plz/pull/1662))

## [0.3.86](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.85...release-plz-v0.3.86) - 2024-09-07

### Added
- add gitlab support for the `release-pr` command ([#1651](https://github.com/release-plz/release-plz/pull/1651))

### Other
- fix inspect_err clippy lint ([#1658](https://github.com/release-plz/release-plz/pull/1658))

## [0.3.85](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.84...release-plz-v0.3.85) - 2024-08-25

### Other
- update dependencies ([#1632](https://github.com/release-plz/release-plz/pull/1632))

## [0.3.84](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.83...release-plz-v0.3.84) - 2024-08-18

### Added
- Add committer and author to changelog ([#1606](https://github.com/release-plz/release-plz/pull/1606))
- Add version of the packages into PR name if they all use the same ([#1603](https://github.com/release-plz/release-plz/pull/1603))

## [0.3.83](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.82...release-plz-v0.3.83) - 2024-08-07

### Other
- extract Project struct into its own file ([#1597](https://github.com/release-plz/release-plz/pull/1597))

## [0.3.82](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.81...release-plz-v0.3.82) - 2024-08-07

### Fixed
- Fixed issue preventing creating GitHub releases ([#1594](https://github.com/release-plz/release-plz/pull/1594))

## [0.3.81](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.80...release-plz-v0.3.81) - 2024-08-05

### Added
- add `git_release_latest` configuration option ([#1588](https://github.com/release-plz/release-plz/pull/1588))

## [0.3.80](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.79...release-plz-v0.3.80) - 2024-08-04

### Added
- *(changelog)* add `remote` to the changelog context ([#1575](https://github.com/release-plz/release-plz/pull/1575))

### Other
- *(deps)* bump gix-attributes from 0.22.2 to 0.22.3 ([#1573](https://github.com/release-plz/release-plz/pull/1573))
- extract `get_changelog` function ([#1574](https://github.com/release-plz/release-plz/pull/1574))

## [0.3.79](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.78...release-plz-v0.3.79) - 2024-07-18

### Fixed
- *(release)* specify package name ([#1560](https://github.com/release-plz/release-plz/pull/1560))

## [0.3.78](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.77...release-plz-v0.3.78) - 2024-07-14

### Fixed
- *(release)* don't try to parse changelog if it doesn't exist ([#1563](https://github.com/release-plz/release-plz/pull/1563))
- *(release)* set changelog path ([#1561](https://github.com/release-plz/release-plz/pull/1561))

## [0.3.77](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.76...release-plz-v0.3.77) - 2024-07-14

### Added
- *(set-version)* support single crate project ([#1553](https://github.com/release-plz/release-plz/pull/1553))

### Other
- *(set-version)* document single crate usage ([#1555](https://github.com/release-plz/release-plz/pull/1555))

## [0.3.76](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.75...release-plz-v0.3.76) - 2024-07-12

### Added
- implement set-version command ([#1546](https://github.com/release-plz/release-plz/pull/1546)) ([#1550](https://github.com/release-plz/release-plz/pull/1550))
- add release_link to context ([#1502](https://github.com/release-plz/release-plz/pull/1502))

### Other
- add missing READMEs ([#1551](https://github.com/release-plz/release-plz/pull/1551))

## [0.3.75](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.74...release-plz-v0.3.75) - 2024-06-30

### Other
- update git-cliff to 2.4.0 ([#1535](https://github.com/release-plz/release-plz/pull/1535))

## [0.3.74](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.73...release-plz-v0.3.74) - 2024-06-21

### Added
- Support for GitLab on-premise, multilevel URLs and SSH git repos ([#1521](https://github.com/release-plz/release-plz/pull/1521))

### Other
- set GitHub and Gitea tokens to be sensitive ([#1529](https://github.com/release-plz/release-plz/pull/1529))

## [0.3.73](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.72...release-plz-v0.3.73) - 2024-06-18

### Fixed
- `release-plz release` command now works if `-o` isn't specified ([#1528](https://github.com/release-plz/release-plz/pull/1528))

### Other
- remove dead code ([#1525](https://github.com/release-plz/release-plz/pull/1525))

## [0.3.72](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.71...release-plz-v0.3.72) - 2024-06-10

### Fixed
- fix([#1487](https://github.com/release-plz/release-plz/pull/1487)): don't close release PR on force-push ([#1513](https://github.com/release-plz/release-plz/pull/1513))

## [0.3.71](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.70...release-plz-v0.3.71) - 2024-06-09

### Fixed
- readme comparison ([#1496](https://github.com/release-plz/release-plz/pull/1496))
- Handle CARGO_REGISTRIES_<name>_TOKEN env with uppercase ([#1507](https://github.com/release-plz/release-plz/pull/1507))

### Other
- add new line to `release-plz.toml` config json schema ([#1500](https://github.com/release-plz/release-plz/pull/1500))

## [0.3.70](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.69...release-plz-v0.3.70) - 2024-05-27

### Other
- update Cargo.toml dependencies

## [0.3.69](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.68...release-plz-v0.3.69) - 2024-05-21

### Added
- add package name to changelog context ([#1399](https://github.com/release-plz/release-plz/pull/1399)) ([#1469](https://github.com/release-plz/release-plz/pull/1469)) ([#1480](https://github.com/release-plz/release-plz/pull/1480))

### Other
- move fixtures directory ([#1467](https://github.com/release-plz/release-plz/pull/1467))

## [0.3.68](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.67...release-plz-v0.3.68) - 2024-05-14

### Fixed
- don't add directories to github api commit ([#1461](https://github.com/release-plz/release-plz/pull/1461))

## [0.3.67](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.66...release-plz-v0.3.67) - 2024-05-12

### Fixed
- updating pr persists GitHub verified status ([#1459](https://github.com/release-plz/release-plz/pull/1459))

### Other
- add more error contexts ([#1455](https://github.com/release-plz/release-plz/pull/1455))
- improve error message when updating pr ([#1453](https://github.com/release-plz/release-plz/pull/1453))

## [0.3.66](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.65...release-plz-v0.3.66) - 2024-05-08

### Added
- print released prs ([#1449](https://github.com/release-plz/release-plz/pull/1449))

### Other
- more descriptive file system error messages ([#1450](https://github.com/release-plz/release-plz/pull/1450))

## [0.3.65](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.64...release-plz-v0.3.65) - 2024-05-05

### Added
- use cargo registry environment variable to authenticate in private sparse registry ([#1435](https://github.com/release-plz/release-plz/pull/1435))

### Other
- add `needless_pass_by_value` lint ([#1441](https://github.com/release-plz/release-plz/pull/1441))
- add `uninlined_format_args` ([#1440](https://github.com/release-plz/release-plz/pull/1440))
- add clippy lints ([#1439](https://github.com/release-plz/release-plz/pull/1439))
- add `if_not_else` clippy lint ([#1438](https://github.com/release-plz/release-plz/pull/1438))
- update dependencies ([#1437](https://github.com/release-plz/release-plz/pull/1437))

## [0.3.64](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.63...release-plz-v0.3.64) - 2024-04-28

### Fixed
- json output of release command ([#1421](https://github.com/release-plz/release-plz/pull/1421))

## [0.3.63](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.62...release-plz-v0.3.63) - 2024-04-25

### Fixed
- json output for empty results ([#1418](https://github.com/release-plz/release-plz/pull/1418))

## [0.3.62](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.61...release-plz-v0.3.62) - 2024-04-25

### Fixed
- use token to fetch sparse metadata ([#1416](https://github.com/release-plz/release-plz/pull/1416))

## [0.3.61](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.60...release-plz-v0.3.61) - 2024-04-21

### Added
- add `release_always` option to release on release PRs only ([#1407](https://github.com/release-plz/release-plz/pull/1407))

### Fixed
- avoid potential deadlock in large repo ([#1408](https://github.com/release-plz/release-plz/pull/1408))

### Other
- fix `if_then_some_else_none` clippy lint ([#1405](https://github.com/release-plz/release-plz/pull/1405))
- don't log cargo stderr ([#1404](https://github.com/release-plz/release-plz/pull/1404))
- improve the github graphql query ([#1383](https://github.com/release-plz/release-plz/pull/1383))

## [0.3.60](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.59...release-plz-v0.3.60) - 2024-04-14

### Fixed
- correctly alias `project-manifest` ([#1396](https://github.com/release-plz/release-plz/pull/1396))

## [0.3.59](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.58...release-plz-v0.3.59) - 2024-04-14

### Other
- Change the flag --project-manifest to --manifest-path [#1333](https://github.com/release-plz/release-plz/pull/1333) ([#1390](https://github.com/release-plz/release-plz/pull/1390)) ([#1395](https://github.com/release-plz/release-plz/pull/1395))

## [0.3.58](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.57...release-plz-v0.3.58) - 2024-04-08

### Added
- add json output ([#1386](https://github.com/release-plz/release-plz/pull/1386)) ([#1368](https://github.com/release-plz/release-plz/pull/1368))

### Other
- document `get_registry_packages` function ([#1381](https://github.com/release-plz/release-plz/pull/1381))

## [0.3.57](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.56...release-plz-v0.3.57) - 2024-04-02

### Fixed
- initialize git repo for downloaded packages ([#1377](https://github.com/release-plz/release-plz/pull/1377))

### Other
- update dependencies ([#1379](https://github.com/release-plz/release-plz/pull/1379))
- *(deps)* bump git-cliff-core to v2 ([#1361](https://github.com/release-plz/release-plz/pull/1361))
- fix cargo-binstall pkg-url ([#1364](https://github.com/release-plz/release-plz/pull/1364))
- emit a warning on readme file mismatch ([#1363](https://github.com/release-plz/release-plz/pull/1363))

## [0.3.56](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.55...release-plz-v0.3.56) - 2024-03-24

### Added
- add `init` command ([#1350](https://github.com/release-plz/release-plz/pull/1350))

### Other
- update dependencies ([#1358](https://github.com/release-plz/release-plz/pull/1358))

## [0.3.55](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.54...release-plz-v0.3.55) - 2024-03-15

### Other
- update Cargo.lock dependencies

## [0.3.54](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.53...release-plz-v0.3.54) - 2024-03-14

### Added
- allow custom `git_release_body` ([#1341](https://github.com/release-plz/release-plz/pull/1341))

### Other
- add getting started ([#1339](https://github.com/release-plz/release-plz/pull/1339))

## [0.3.53](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.52...release-plz-v0.3.53) - 2024-03-10

### Fixed
- allow rust project in subdir ([#1336](https://github.com/release-plz/release-plz/pull/1336))

### Other
- use `camino` ([#1337](https://github.com/release-plz/release-plz/pull/1337))
- improve error message to suggest `--project-manifest` ([#1334](https://github.com/release-plz/release-plz/pull/1334))

## [0.3.52](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.51...release-plz-v0.3.52) - 2024-03-10

### Added
- create annotated tags instead of lightweight ([#1255](https://github.com/release-plz/release-plz/pull/1255))

## [0.3.51](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.50...release-plz-v0.3.51) - 2024-03-08

### Fixed
- check diffs in Cargo.toml custom README path ([#1315](https://github.com/release-plz/release-plz/pull/1315))

### Other
- use edition and license workspace ([#1329](https://github.com/release-plz/release-plz/pull/1329))

## [0.3.50](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.49...release-plz-v0.3.50) - 2024-03-06

### Added
- Add `publish_features` to pass feature list to `cargo publish` ([#1320](https://github.com/release-plz/release-plz/pull/1320))

## [0.3.49](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.48...release-plz-v0.3.49) - 2024-02-27

### Other
- error if committed Cargo.lock is in `.gitignore` ([#1294](https://github.com/release-plz/release-plz/pull/1294))

## [0.3.48](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.47...release-plz-v0.3.48) - 2024-02-25

### Other
- update Cargo.lock dependencies
- don't log big release request ([#1300](https://github.com/release-plz/release-plz/pull/1300))

## [0.3.47](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.46...release-plz-v0.3.47) - 2024-02-25

### Added
- prepare release if commits respect the `release_commits` regex ([#1278](https://github.com/release-plz/release-plz/pull/1278))

### Other
- update cargo to v0.77 ([#1296](https://github.com/release-plz/release-plz/pull/1296))

## [0.3.46](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.45...release-plz-v0.3.46) - 2024-02-23

### Added
- add `all-static` feature ([#1287](https://github.com/release-plz/release-plz/pull/1287))

### Fixed
- allow configuring the `release` flag ([#1290](https://github.com/release-plz/release-plz/pull/1290))

### Other
- enhance test `schema_is_up_to_date` ([#1285](https://github.com/release-plz/release-plz/pull/1285))

## [0.3.45](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.44...release-plz-v0.3.45) - 2024-02-11

### Added
- allow customizing git release name with tera template. [#677](https://github.com/release-plz/release-plz/pull/677) ([#1260](https://github.com/release-plz/release-plz/pull/1260))

### Fixed
- *(config)* deny unknown fields ([#1263](https://github.com/release-plz/release-plz/pull/1263))

## [0.3.44](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.43...release-plz-v0.3.44) - 2024-02-09

### Added
- allow customizing git tag name with tera template ([#1256](https://github.com/release-plz/release-plz/pull/1256))

## [0.3.43](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.42...release-plz-v0.3.43) - 2024-02-06

### Added
- add changelog config in `release-plz.toml` ([#1198](https://github.com/release-plz/release-plz/pull/1198))

### Fixed
- update local dependencies specified in the workspace manifest ([#1251](https://github.com/release-plz/release-plz/pull/1251))
- check cargo token only when publishing ([#1250](https://github.com/release-plz/release-plz/pull/1250))

### Other
- fix tests on mac ([#1242](https://github.com/release-plz/release-plz/pull/1242))

## [0.3.42](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.41...release-plz-v0.3.42) - 2024-01-26

### Added
- add `git_release_type` configuration option to allow GitHub/Gitea prereleases ([#1228](https://github.com/release-plz/release-plz/pull/1228))

### Fixed
- support rust-toolchain.toml file ([#1234](https://github.com/release-plz/release-plz/pull/1234))

### Other
- add context to some errors ([#1232](https://github.com/release-plz/release-plz/pull/1232))

## [0.3.41](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.40...release-plz-v0.3.41) - 2024-01-23

### Added
- use github graphql api for commits to have the GitHub "Verified" badge on release-plz commits
  ([#1201](https://github.com/release-plz/release-plz/pull/1201))

### Other
- update Cargo.lock dependencies

## [0.3.40](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.39...release-plz-v0.3.40) - 2024-01-20

### Fixed
- Correct dependency update behavior for release-pr ([#1217](https://github.com/release-plz/release-plz/pull/1217))

### Other
- update dependencies ([#1213](https://github.com/release-plz/release-plz/pull/1213))

## [0.3.39](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.38...release-plz-v0.3.39) - 2024-01-16

### Added
- Add release flag ([#1098](https://github.com/release-plz/release-plz/pull/1098))

### Fixed
- Prevent error if Cargo.lock doesn't exist during portions of commit history ([#1205](https://github.com/release-plz/release-plz/pull/1205))

### Other
- improve public packages error message ([#1187](https://github.com/release-plz/release-plz/pull/1187))
- add debug statement when Cargo.toml differs ([#1184](https://github.com/release-plz/release-plz/pull/1184))
- less verbose logs ([#1183](https://github.com/release-plz/release-plz/pull/1183))

## [0.3.38](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.37...release-plz-v0.3.38) - 2023-12-30

### Other
- flatten part of config struct ([#1154](https://github.com/release-plz/release-plz/pull/1154))
- remove unuseful function ([#1166](https://github.com/release-plz/release-plz/pull/1166))
- simplify code ([#1165](https://github.com/release-plz/release-plz/pull/1165))

## [0.3.37](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.36...release-plz-v0.3.37) - 2023-12-19

### Fixed
- restore changes introduced by `cargo package` ([#1152](https://github.com/release-plz/release-plz/pull/1152))

## [0.3.36](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.35...release-plz-v0.3.36) - 2023-12-17

### Other
- update dependencies

## [0.3.35](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.34...release-plz-v0.3.35) - 2023-12-16

### Added
- return error if tag exists and package isn't published ([#1143](https://github.com/release-plz/release-plz/pull/1143))
- support packages with git dependencies ([#1141](https://github.com/release-plz/release-plz/pull/1141))
- distinguish dependency update type ([#1140](https://github.com/release-plz/release-plz/pull/1140))

### Fixed
- internal dependency conflict ([#1135](https://github.com/release-plz/release-plz/pull/1135))

### Other
- update dependencies
- *(deps)* bump cargo to 0.75.1 ([#1137](https://github.com/release-plz/release-plz/pull/1137))

## [0.3.34](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.33...release-plz-v0.3.34) - 2023-12-13

### Fixed
- pass previous version to git-cliff ([#1134](https://github.com/release-plz/release-plz/pull/1134))

### Other
- update dependencies

## [0.3.33](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.32...release-plz-v0.3.33) - 2023-12-08

### Added
- new generate-schema command to generate a JSON schema for the configuration ([#1101](https://github.com/release-plz/release-plz/pull/1101))

### Other
- *(args)* hide the environment value of git token ([#1124](https://github.com/release-plz/release-plz/pull/1124))
- update git-cliff references ([#1115](https://github.com/release-plz/release-plz/pull/1115))

## [0.3.32](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.31...release-plz-v0.3.32) - 2023-12-04

### Fixed
- support projects with external readme ([#1110](https://github.com/release-plz/release-plz/pull/1110))
- pass full commit message to git-cliff ([#1103](https://github.com/release-plz/release-plz/pull/1103)) ([#1104](https://github.com/release-plz/release-plz/pull/1104))

### Other
- update dependencies

## [0.3.31](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.30...release-plz-v0.3.31) - 2023-11-30

### Added
- add publish_timeout to avoid release blocking issue, fix [#1015](https://github.com/release-plz/release-plz/pull/1015) ([#1088](https://github.com/release-plz/release-plz/pull/1088))
- prevent typos in overrides ([#1080](https://github.com/release-plz/release-plz/pull/1080))
- Update a package only if edited file belongs to `cargo package --list` ([#1089](https://github.com/release-plz/release-plz/pull/1089))

### Fixed
- resolve issue on Windows machines that use CRLF that would duplicate the header on each update ([#1083](https://github.com/release-plz/release-plz/pull/1083))

### Other
- document Gitea releases ([#1076](https://github.com/release-plz/release-plz/pull/1076))

## [0.3.30](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.29...release-plz-v0.3.30) - 2023-11-08

### Added
- add `pr_draft` config option ([#1061](https://github.com/release-plz/release-plz/pull/1061))
- support .release-plz.toml as a config file ([#1057](https://github.com/release-plz/release-plz/pull/1057))

## [0.3.29](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.28...release-plz-v0.3.29) - 2023-10-27

### Fixed
- use `release-plz-` rather than `release-plz/` branch prefix ([#1041](https://github.com/release-plz/release-plz/pull/1041))
- use registry argument on publish ([#1050](https://github.com/release-plz/release-plz/pull/1050))

## [0.3.28](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.27...release-plz-v0.3.28) - 2023-10-15

### Added
- *(args)* support `GIT_TOKEN` variable ([#1008](https://github.com/release-plz/release-plz/pull/1008)) ([#1026](https://github.com/release-plz/release-plz/pull/1026))

### Fixed
- ignore `.ignore` files ([#1036](https://github.com/release-plz/release-plz/pull/1036))

## [0.3.27](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.26...release-plz-v0.3.27) - 2023-09-30

### Other
- update dependencies

## [0.3.26](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.25...release-plz-v0.3.26) - 2023-09-30

### Added
- avoid copying gitignored files ([#1000](https://github.com/release-plz/release-plz/pull/1000)) ([#1001](https://github.com/release-plz/release-plz/pull/1001))

### Fixed
- parse changelog header correctly ([#1007](https://github.com/release-plz/release-plz/pull/1007))

### Other
- update dependencies

## [0.3.25](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.24...release-plz-v0.3.25) - 2023-09-24

### Added
- add ability to create draft git release ([#986](https://github.com/release-plz/release-plz/pull/986))

### Fixed
- respect git-cliff sort order ([#985](https://github.com/release-plz/release-plz/pull/985))

## [0.3.24](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.23...release-plz-v0.3.24) - 2023-09-17

### Fixed
- don't discard error context ([#971](https://github.com/release-plz/release-plz/pull/971))
- don't publish examples ([#974](https://github.com/release-plz/release-plz/pull/974))

## [0.3.23](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.22...release-plz-v0.3.23) - 2023-09-16

### Added
- allow disabling git tag ([#968](https://github.com/release-plz/release-plz/pull/968))
- pass commit ids to git-cliff ([#967](https://github.com/release-plz/release-plz/pull/967))

### Other
- add additional clippy lints ([#965](https://github.com/release-plz/release-plz/pull/965))

## [0.3.22](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.21...release-plz-v0.3.22) - 2023-09-11

### Added
- *(release-pr)* sign release-plz commit ([#956](https://github.com/release-plz/release-plz/pull/956))

## [0.3.21](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.20...release-plz-v0.3.21) - 2023-09-08

### Other
- update dependencies
- *(ci)* check links ([#941](https://github.com/release-plz/release-plz/pull/941))
- fix clippy lint ([#931](https://github.com/release-plz/release-plz/pull/931))

## [0.3.20](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.19...release-plz-v0.3.20) - 2023-08-22

### Fixed
- allow specifying config file path ([#924](https://github.com/release-plz/release-plz/pull/924))

### Other
- test release-plz release ([#892](https://github.com/release-plz/release-plz/pull/892))
- move release-plz changelog ([#917](https://github.com/release-plz/release-plz/pull/917))
- add feature flag to ignore docker tests ([#914](https://github.com/release-plz/release-plz/pull/914))
- static openssl ([#920](https://github.com/release-plz/release-plz/pull/920))
- improve http error messages ([#921](https://github.com/release-plz/release-plz/pull/921))
- update git-cliff ([#919](https://github.com/release-plz/release-plz/pull/919))

## [0.3.19](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.18...release-plz-v0.3.19) - 2023-08-16

### Fixed
- release in https git repos ([#912](https://github.com/release-plz/release-plz/pull/912))

## [0.3.18](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.17...release-plz-v0.3.18) - 2023-08-14

### Added
- add `changelog_include` option ([#904](https://github.com/release-plz/release-plz/pull/904))

### Other
- add tests for gitea ([#421](https://github.com/release-plz/release-plz/pull/421))

## [0.3.17](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.16...release-plz-v0.3.17) - 2023-08-02

### Fixed
- update workspace version in dependencies ([#889](https://github.com/release-plz/release-plz/pull/889))

## [0.3.16](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.15...release-plz-v0.3.16) - 2023-07-25

### Added
- *(release)* add support for sparse registry URLs ([#863](https://github.com/release-plz/release-plz/pull/863))

## [0.3.15](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.14...release-plz-v0.3.15) - 2023-06-26

### Fixed
- copy symlinks ([#827](https://github.com/release-plz/release-plz/pull/827))

## [0.3.14](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.13...release-plz-v0.3.14) - 2023-06-10

### Fixed
- ignore Cargo.lock dev dependencies changes ([#820](https://github.com/release-plz/release-plz/pull/820))

## [0.3.13](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.12...release-plz-v0.3.13) - 2023-06-09

### Fixed
- update changelog correctly when workspace version specified ([#816](https://github.com/release-plz/release-plz/pull/816))

## [0.3.12](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.11...release-plz-v0.3.12) - 2023-06-09

- update dependencies ([#814](https://github.com/release-plz/release-plz/pull/814))
- stop looking at git history if commit tagged ([#813](https://github.com/release-plz/release-plz/pull/813))

## [0.3.11](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.10...release-plz-v0.3.11) - 2023-05-31

### Fixed
- *(workspaces)* report correct version update ([#802](https://github.com/release-plz/release-plz/pull/802))

## [0.3.10](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.9...release-plz-v0.3.10) - 2023-05-24

### Added
- add pr/issue link to changelog entries (#793)

### Other
- parse cargo lock faster (#795)

## [0.3.9](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.8...release-plz-v0.3.9) - 2023-05-21

### Added
- check if `Cargo.lock` packages were updated (#784)

### Fixed
- support nested crates (#783)

## [0.3.8](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.7...release-plz-v0.3.8) - 2023-05-08

### Other
- run cargo-semver-check in parallel (#766)
- represent semver_check config as bool (#765)

## [0.3.7](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.6...release-plz-v0.3.7) - 2023-05-07

### Other
- Performance improvement: run semver-checks only on changed packages (#754)

## [0.3.6](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.5...release-plz-v0.3.6) - 2023-05-07

### Fixed
- abort failed rebase (#760)

## [0.3.5](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.4...release-plz-v0.3.5) - 2023-05-05

### Fixed
- set repo url also for release command (#751)

### Added
- Add `publish` config option to disable publishing to the cargo registry (#718)

## [0.3.4](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.3...release-plz-v0.3.4) - 2023-04-27

### Fixed
- don't compare ignored files (#739)

## [0.3.3](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.2...release-plz-v0.3.3) - 2023-04-25

### Fixed
- downgrade cargo to fix windows compilation

## [0.3.2](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.1...release-plz-v0.3.2) - 2023-04-24

### Other
- only add commit title in changelog (#729)

## [0.3.1](https://github.com/release-plz/release-plz/compare/release-plz-v0.3.0...release-plz-v0.3.1) - 2023-04-21

### Added
- add `pr_labels` configuration option to add labels to the PR opened by release-plz (#708)

## [0.3.0](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.63...release-plz-v0.3.0) - 2023-04-16

### Added
- [**breaking**] changed config file format. See [docs](https://release-plz.dev/docs/config.html).
- [**breaking**] removed `--git-release` flag. Now git releases are enabled by default.
  You can disable them with the `git_release_enable` configuration option.
- make cargo publish flags configurable (#684)

### Fixed
- config package override (#695)
- don't return early when publishing crates (#691)

## [0.2.63](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.62...release-plz-v0.2.63) - 2023-04-05

### Fixed
- changelog path handling (#669)
- detect allow-dirty error (#666)

## [0.2.62](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.61...release-plz-v0.2.62) - 2023-04-02

### Added
- allow to provide a custom changelog path (#653)

## [0.2.61](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.60...release-plz-v0.2.61) - 2023-04-02

### Other
- detect custom changelog header (#651)

## [0.2.60](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.59...release-plz-v0.2.60) - 2023-04-02

### Other
- read opened PRs with empty body (#649)

## [0.2.59](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.58...release-plz-v0.2.59) - 2023-04-01

### Added
- Add config file. See the [docs](https://release-plz.dev/docs/config.html) (#634)

## [0.2.58](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.57...release-plz-v0.2.58) - 2023-03-27

### Added
- add release-plz config file (#589). Experimental, not documented yet.

## [0.2.57](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.56...release-plz-v0.2.57) - 2023-03-19

### Added
- *(release)* add GitLab support (#591).
  `release-plz release-pr` GitLab support is still missing.

## [0.2.56](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.55...release-plz-v0.2.56) - 2023-03-17

### Fixed
- update pr: do git fetch before rebase (#607)

## [0.2.55](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.54...release-plz-v0.2.55) - 2023-03-13

### Added
- write changelog in pr body (#598)

## [0.2.54](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.53...release-plz-v0.2.54) - 2023-03-10

### Fix
- update to cargo v0.69 to be compatible with sparse index.

## [0.2.53](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.52...release-plz-v0.2.53) - 2023-03-09

### Added
- include version in pr title for single crate (#593)

### Other
- retry failing http calls (#585)

## [0.2.52](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.51...release-plz-v0.2.52) - 2023-03-04

### Added
- detect circular dependency (#581)

## [0.2.51](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.50...release-plz-v0.2.51) - 2023-02-27

### Fix
- remove ansi escape sequences in cargo-semver-checks output (#575)

## [0.2.50](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.49...release-plz-v0.2.50) - 2023-02-26

### Added
- Add cargo-semver-checks integration. If the `cargo-semver-checks` binary is present, release-plz uses
  it to check semver compatibility. If `cargo-semver-checks` detects an API breaking change, release-plz
  updates the major version. (#568)

### Fixed
- when editing a release-pr, update pr title and body (#571)

## [0.2.49](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.48...release-plz-v0.2.49) - 2023-02-20

### Other
- update dependencies
- remove unused check (#559)

## [0.2.48](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.47...release-plz-v0.2.48) - 2023-02-18

### Fixed
- *(release)* fix github release (#556)

## [0.2.47](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.46...release-plz-v0.2.47) - 2023-02-18

### Fixed
- *(release)* trust gh workspace (#553)

## [0.2.46](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.45...release-plz-v0.2.46) - 2023-02-18

### Other
- `release-plz release` creates a release in Gitea, too (#539)

## [0.2.45](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.44...release-plz-v0.2.45) - 2023-02-11

### Added
- *(release)* add the possibility to add `--no-verify` and `--allow-dirty` as cargo publish flags (#532)

## [0.2.44](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.43...release-plz-v0.2.44) - 2023-02-10

### Added
- update pr in gitea (#530)

### Fixed
- update branch from main before updating PR (#528)

## [0.2.43](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.42...release-plz-v0.2.43) - 2023-02-08

### Added
- add changelog changes to gitea (#525)
- log published version (#514)

## [0.2.42](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.41...release-plz-v0.2.42) - 2023-01-31

### Other
- *(release)* trust github workspace dir (#512)

## [0.2.41](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.40...release-plz-v0.2.41) - 2023-01-31

### Fixed
- handle new crate correctly (#509, #511)

### Other
- improve log (#502)

## [0.2.40](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.39...release-plz-v0.2.40) - 2023-01-27

### Fixed
- fix edit pr when a new file is present (#498)

### Other
- improve logging (#500)

## [0.2.39](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.38...release-plz-v0.2.39) - 2023-01-26

### Added
- edit GitHub release pr instead of closing it (#470)

### Other
- fix cargo clippy (#489)

## [0.2.38](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.37...release-plz-v0.2.38) - 2023-01-22

### Added
- add new `check-updates` command to check if release-plz is up to date (#477) (#471)

## [0.2.37](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.36...release-plz-v0.2.37) - 2023-01-22

### Other
- update cargo (#473)

## [0.2.36](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.35...release-plz-v0.2.36) - 2023-01-17

### Other
- remove `octocrab` dependency (#467)

## [0.2.35](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.34...release-plz-v0.2.35) - 2023-01-16

### Added
- *(release-pr)* do not include the crate name if there is only one
  publishable package in the project (#461)

### Fixed
- wrong log line (#464)

## [0.2.34](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.33...release-plz-v0.2.34) - 2023-01-16

### Fixed
- do not update changelog if new version exists (#452)
- changelog: fix link to first change (#450)

### Other
- *(deps)* bump assert_cmd from 2.0.7 to 2.0.8 (#453)

## [0.2.33](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.32...release-plz-v0.2.33) - 2023-01-15

### Added
- do not prefix crate name in tag for single crate projects (#444)

## [0.2.32](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.31...release-plz-v0.2.32) - 2023-01-12

### Added
- Include previous version in Pr Body (#430)

## [0.2.31](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.30...release-plz-v0.2.31) - 2023-01-11

### Added
- don't remove build metadata (#433)
- handle pre-releases (#425)

## [0.2.30](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.29...release-plz-v0.2.30) - 2023-01-07

### Added
- add body to git release (#411)

### Fixed
- *(release)* git-token is optional (#413)

## [0.2.29](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.28...release-plz-v0.2.29) - 2023-01-06

### Added
- Initial support for GitHub releases. You can create a GitHub release when running `release-plz release` by using the `--git-release` flag.

### Other
- print error kind when copying directories (#408)
- make errors more visible (#405)
- use secret strings for tokens (#403)

## [0.2.28](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.27...release-plz-v0.2.28) - 2022-12-26

### Fixed
- reintroduce github-token flag (#389)

## [0.2.27](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.26...release-plz-v0.2.27) - 2022-12-16

### Other
- Add support for Gitea repositories (#372)

## [0.2.26](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.25...release-plz-v0.2.26) - 2022-12-12

### Added
- Changelog: add GitHub release link to show the commits since the previous version (#371)

### Other
- *(deps)* bump assert_cmd from 2.0.6 to 2.0.7 (#366)
- *(completions)* add tests for shell completions (#330) (#349)

## [0.2.25](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.24...release-plz-v0.2.25) - 2022-11-17

### Changed
- don't release if the tag exists (#342)

## [0.2.24](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.23...release-plz-v0.2.24) - 2022-11-12

### Fixed
- breaking remove deprecated chrono::Date (#340)

## [0.2.23](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.22...release-plz-v0.2.23) - 2022-11-04

### Fixed
- github token parsing (#334)

### Other
- use workspace dependencies (#333)

## [0.2.22](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.21...release-plz-v0.2.22) - 2022-11-03

### Fixed
- *(args)* use the correct case for conflicting arguments (#328)

## [0.2.21](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.20...release-plz-v0.2.21) - 2022-11-01

### Other
- update to clap v4 (#325)

## [0.2.20](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.19...release-plz-v0.2.20) - 2022-10-24

### Other
- bump dependencies

## [0.2.19](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.18...release-plz-v0.2.19) - 2022-07-16

### Other
- update git cliff to version 8 (#212)
- *(deps)* bump clap_complete from 3.2.2 to 3.2.3 (#201)
- *(deps)* bump clap from 3.2.6 to 3.2.8 (#200)
- *(deps)* bump tracing-subscriber from 0.3.11 to 0.3.14 (#199)

## [0.2.18](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.17...release-plz-v0.2.18) - 2022-06-18

### Added
- add `generate-completions` command to generate shell completions file. (#177)

### Other
- *(deps)* bump clap and fake libraries (#186)
- *(deps)* bump tracing from 0.1.34 to 0.1.35 (#179)
- *(deps)* bump tokio from 1.19.1 to 1.19.2 (#178)
- *(deps)* bump tokio from 1.18.2 to 1.19.1 (#175)
- *(deps)* bump git-url-parse from 0.4.1 to 0.4.2 (#172)

## [0.2.17](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.16...release-plz-v0.2.17) - 2022-05-29

### Added
- add --allow-dirty flag to update command (#169)

## [0.2.16](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.15...release-plz-v0.2.16) - 2022-05-29

### Added
- add `verbose` flag (#167)

## [0.2.15](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.14...release-plz-v0.2.15) - 2022-05-28

### Other
- skip pr field in logs (#165)

## [0.2.14](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.13...release-plz-v0.2.14) - 2022-05-28

### Other
- update dependencies (#160)

## [0.2.13](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.12...release-plz-v0.2.13) - 2022-05-28

### Other
- updated the following local packages: release_plz_core

## [0.2.12](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.11...release-plz-v0.2.12) - 2022-05-26

### Other
- improve PR body (#139)

## [0.2.11](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.10...release-plz-v0.2.11) - 2022-05-19

### Other
- upgrade dependencies (#133)

## [0.2.10](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.9...release-plz-v0.2.10) - 2022-05-14

### Added
- infer repo url (#128)

## [0.2.9](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.8...release-plz-v0.2.9) - 2022-05-13

### Added
- read custom git cliff config (#126)

## [0.2.8](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.7...release-plz-v0.2.8) - 2022-05-10

### Added
- add ability to update all the dependencies in the lockfile with the `-u` cli option (#123)

## [0.2.7](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.6...release-plz-v0.2.7) - 2022-05-08

### Other
- update package if one of its local dependencies is updated (#112)

## [0.2.6](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.5...release-plz-v0.2.6) - 2022-05-02

### Changed
- update `release_plz_core` to 0.2.7

## [0.2.5](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.4...release-plz-v0.2.5) - 2022-05-01

### Changed
- update `release_plz_core` to 0.2.6

## [0.2.4](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.3...release-plz-v0.2.4) - 2022-04-27

### Added
- add `release` command (#89)
- *(cli)* forbid empty values in args (#88)

### Other
- *(args)* refactor (#87)

## [0.2.3](https://github.com/release-plz/release-plz/compare/release-plz-v0.2.2...release-plz-v0.2.3) - 2022-04-23

### Added
- *(release-pr)* close old release-plz prs when running release-plz (#81)
- update Cargo.lock, too (#78)

## [0.2.2](https://github.com/release-plz/release-plz/releases/tag/release-plz-v0.2.2) - 2022-04-10

### Fixed
- remove all unwraps that are not in tests (#49)

## [0.2.1] - 2022-03-30

### Added
- support alternative registries (#34)

### Other
- update crate description

## [0.2.0] - 2022-03-27

### Added
- [**breaking**] generate changelog with git-cliff (#29)
