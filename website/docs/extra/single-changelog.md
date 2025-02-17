# Single changelog

## One package

If you have a workspace with multiple packages, and you want to keep
track of the changes of just one package, you can customize your
`release-plz.toml` file like this:

```toml
[workspace]
# disable the changelog for all packages
changelog_update = false

[[package]]
name = "my-important-package"
# enable the changelog for this package
changelog_update = true
# set the path of the changelog to the root of the repository
changelog_path = "./CHANGELOG.md"
```

To include commits of other packages in the changelog of
your main package, use the
[changelog_include](../config.md#the-changelog_include-field) field.

## All packages

If you have a workspace with multiple packages, and you want to group all the
changes in a single changelog, you can customize your `release-plz.toml`
file like this:

```toml
[workspace]
# set the path of all the crates to the changelog to the root of the repository
changelog_path = "./CHANGELOG.md"

[changelog]
body = """

## `{{ package }}` - [{{ version | trim_start_matches(pat="v") }}]{%- if release_link -%}({{ release_link }}){% endif %} - {{ timestamp | date(format="%Y-%m-%d") }}
{% for group, commits in commits | group_by(attribute="group") %}
### {{ group | upper_first }}
{% for commit in commits %}
{%- if commit.scope -%}
- *({{commit.scope}})* {% if commit.breaking %}[**breaking**] {% endif %}{{ commit.message }}{%- if commit.links %} ({% for link in commit.links %}[{{link.text}}]({{link.href}}) {% endfor -%}){% endif %}
{% else -%}
- {% if commit.breaking %}[**breaking**] {% endif %}{{ commit.message }}
{% endif -%}
{% endfor -%}
{% endfor -%}
"""
```

The difference with the default changelog body configuration is that the header now also contains
the `{{package}}`.

The entries will be order depending on the release order: if `package_a` depends on `package_b`,
the changes of `package_a` are listed after `package_b`.
For example, if you have a workspace with various libraries and one binary that depends on all the
libraries, the changes of the binary will be placed first.

In this way, `package_a` and `package_b` changelogs are in the same file.
Note that the changelog will contain duplicate changes.
If you want to merge updates of different packages into one, check
the [changelog_include](../config.md#the-changelog_include-field) field.

Here's an example of the changelog output:

```md
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## `marco-test-two` - [0.4.20](https://github.com/marcoieni/rust-workspace-example/compare/marco-test-two-v0.4.19...marco-test-two-v0.4.20) - 2024-05-17

### Added
- new feature

## `marco-test-three` - [0.1.16](https://github.com/marcoieni/rust-workspace-example/compare/marco-test-three-v0.1.15...marco-test-three-v0.1.16) - 2024-05-17

### Other
- Small change
```

:::tip
You can enable the changelog for a subset of packages only:

```toml
[workspace]
# Disable the changelog for all packages.
changelog_update = false
changelog_path = "./CHANGELOG.md"

[[package]]
name = "package_a"
# Enable the changelog for this package (override default).
changelog_update = true
```

:::

:::tip
You can customize the changelog path for each package.
In the following example, the changes of `package_b` will be added to its own changelog,
instead of being included in `./CHANGELOG.md` like all the other packages.

```toml
[workspace]
changelog_path = "./CHANGELOG.md"

[[package]]
name = "package_b"
changelog_path = "package_b/CHANGELOG.md"
```

:::
