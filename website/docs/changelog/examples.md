# Examples

Release-plz comes with a default changelog configuration that adheres to the
[Keep a Changelog](https://keepachangelog.com/en/1.0.0/) specification.
You can customize the changelog format by editing the
[`[changelog]`](../config.md#the-changelog-section) section of the configuration file.

In the following there are some examples of changelog configurations that you can
use to take inspiration from. ✨

If you want to contribute your cool template,
[open a PR](https://github.com/release-plz/release-plz/blob/main/CONTRIBUTING.md)! 🙏

:::info
All examples based on the following [Git
history](https://github.com/orhun/git-cliff-readme-example):

```text
* df6aef4 (HEAD -> master) feat(cache): use cache while fetching pages
* a9d4050 feat(config): support multiple file formats
* 06412ac (tag: v1.0.1) chore(release): add release script
* e4fd3cf refactor(parser): expose string functions
* ad27b43 (tag: v1.0.0) docs(example)!: add tested usage example
* 9add0d4 fix(args): rename help argument due to conflict
* a140cef feat(parser): add ability to parse arrays
* 81fbc63 docs(project): add README.md
* a78bc36 Initial commit
```

:::

## Release-plz default

Release-plz default configuration, purely here as a reference.

<details>
  <summary>TOML configuration</summary>

```toml
[changelog]
header = """# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
"""

body = """

## [{{ version | trim_start_matches(pat="v") }}]\
    {%- if release_link -%}\
        ({{ release_link }})\
    {% endif %} \
    - {{ timestamp | date(format="%Y-%m-%d") }}
{% for group, commits in commits | group_by(attribute="group") %}
### {{ group | upper_first }}

    {% for commit in commits %}
        {%- if commit.scope -%}
            - *({{commit.scope}})* {% if commit.breaking %}[**breaking**] {% endif %}\
                {{ commit.message }}\
                {%- if commit.links %} \
                    ({% for link in commit.links %}[{{link.text}}]({{link.href}}) {% endfor -%})\
                {% endif %}
        {% else -%}
            - {% if commit.breaking %}[**breaking**] {% endif %}{{ commit.message }}
        {% endif -%}
    {% endfor -%}
{% endfor %}
"""

commit_parsers = [
  { message = "^feat", group = "added" },
  { message = "^changed", group = "changed" },
  { message = "^deprecated", group = "deprecated" },
  { message = "^fix", group = "fixed" },
  { message = "^security", group = "security" },
  { message = "^.*", group = "other" },
]
```

</details>

<details>
  <summary>Raw Output</summary>

```md
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.1](https://github.com/orhun/git-cliff-readme-example/compare/v1.0.0...v1.0.1) - 2021-07-18

### Added

- Add release script

### Changed

- Expose string functions

## [1.0.0] - 2021-07-18

### Added

- Add README.md
- Add ability to parse arrays
- Add tested usage example

### Fixed

- Rename help argument due to conflict
```

</details>

<details>
  <summary>Rendered Output</summary>

```mdx-code-block
# Changelog
All notable changes to this project will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
## [Unreleased]
## [1.0.1](https://github.com/orhun/git-cliff-readme-example/compare/v1.0.0...v1.0.1) - 2021-07-18
### Added
- Add release script
### Changed
- Expose string functions
## [1.0.0] - 2021-07-18
### Added
- Add README.md
- Add ability to parse arrays
- Add tested usage example
### Fixed
- Rename help argument due to conflict
```

</details>

## Styled and scoped

<details>
  <summary>TOML configuration</summary>

```toml
[changelog]
header = """# Changelog

## [Unreleased]
"""

body = """

{% macro print_commit(commit) -%}
    - {% if commit.scope %}*({{ commit.scope }})* {% endif %}\
      {% if commit.breaking %}[**breaking**] {% endif %}\
      {{ commit.message | upper_first }} - \
      ([{{ commit.id | truncate(length=7, end="") }}]({{ remote.link }}/commit/{{ commit.id }}))\
{% endmacro -%}

{% if version %}\
    {% if previous.version %}\
        ## [{{ version | trim_start_matches(pat="v") }}]({{ release_link }})
    {% else %}\
        ## [{{ version | trim_start_matches(pat="v") }}]
    {% endif %}\
{% endif %}\

{% for group, commits in commits
| filter(attribute="merge_commit", value=false)
| unique(attribute="message")
| group_by(attribute="group") %}
    ### {{ group | striptags | trim | upper_first }}

    {% for commit in commits
    | filter(attribute="scope")
    | sort(attribute="scope") %}
        {{ self::print_commit(commit=commit) }}
    {%- endfor -%}
    {% raw %}\n{% endraw %}\
    {%- for commit in commits %}
        {%- if not commit.scope -%}
            {{ self::print_commit(commit=commit) }}
        {% endif -%}
    {% endfor -%}
{% endfor %}\n
"""

commit_parsers = [
  { message = "^feat", group = "<!-- 0 -->⛰️ Features" },
  { message = "^fix", group = "<!-- 1 -->🐛 Bug Fixes" },
  { message = "^doc", group = "<!-- 3 -->📚 Documentation" },
  { message = "^perf", group = "<!-- 4 -->⚡ Performance" },
  { message = "^refactor\\(clippy\\)", skip = true },
  { message = "^refactor", group = "<!-- 2 -->🚜 Refactor" },
  { message = "^style", group = "<!-- 5 -->🎨 Styling" },
  { message = "^test", group = "<!-- 6 -->🧪 Testing" },
  { message = "^chore\\(release\\):", skip = true },
  { message = "^chore: release", skip = true },
  { message = "^chore\\(deps.*\\)", skip = true },
  { message = "^chore\\(pr\\)", skip = true },
  { message = "^chore\\(pull\\)", skip = true },
  { message = "^chore\\(npm\\).*yarn\\.lock", skip = true },
  { message = "^chore|^ci", group = "<!-- 7 -->⚙️ Miscellaneous Tasks" },
  { body = ".*security", group = "<!-- 8 -->🛡️ Security" },
  { message = "^revert", group = "<!-- 9 -->◀️ Revert" },
]

link_parsers = [
  { pattern = "#(\\d+)", href = "{{ remote.link }}/issues/$1" },
  { pattern = "RFC(\\d+)", text = "ietf-rfc$1", href = "https://datatracker.ietf.org/doc/html/rfc$1" },
]
```

</details>

<details>
  <summary>Raw Output</summary>

```md
# Changelog

## [Unreleased]

## [1.0.1](https://github.com/orhun/git-cliff-readme-example/compare/v1.0.0...v1.0.1)

### 🚜 Refactor

- refactor(parser): expose string functions ([e4fd3cf](e4fd3cf8e2e6f49c0b57f66416e886c37cbb3715))

### ⚙️ Miscellaneous Tasks

- chore(release): add release script ([06412ac](06412ac1dd4071006c465dde6597a21d4367a158))

## [1.0.0] - 2021-07-18

### ⛰️  Features

- feat(parser): add ability to parse arrays ([a140cef](a140cef0405e0bcbfb5de44ff59e091527d91b38))

### 🐛 Bug Fixes

- fix(args): rename help argument due to conflict ([9add0d4](9add0d4616dc95a6ea8b01d5e4d233876b6e5e00))

### 📚 Documentation

- docs(project): add README.md ([81fbc63](81fbc6365484abf0b4f4b05d384175763ad8db44))
- docs(example)!: add tested usage example ([ad27b43](ad27b43e8032671afb4809a1a3ecf12f45c60e0e))
```

</details>

<details>
  <summary>Rendered Output</summary>

```mdx-code-block
# Changelog
All notable changes to this project will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
## [Unreleased]
## [1.0.1](https://github.com/orhun/git-cliff-readme-example/compare/v1.0.0...v1.0.1)
### 🚜 Refactor
- refactor(parser): expose string functions (e4fd3cf)
### ⚙️ Miscellaneous Tasks
- chore(release): add release script (06412ac)
## [1.0.0] - 2021-07-18
### ⛰️  Features
- feat(parser): add ability to parse arrays (a140cef)
### 🐛 Bug Fixes
- fix(args): rename help argument due to conflict (9add0d4)
### 📚 Documentation
- docs(project): add README.md (81fbc63)
- docs(example)!: add tested usage example (ad27b43)
```

</details>

## Release-plz default + contributors

Like Release-plz default configuration, but it also shows the
GitHub/Gitea/GitLab username of the contributors.

<details>
  <summary>TOML configuration</summary>

```toml
[changelog]
body = """

## [{{ version | trim_start_matches(pat="v") }}]\
    {%- if release_link -%}\
        ({{ release_link }})\
    {% endif %} \
    - {{ timestamp | date(format="%Y-%m-%d") }}
{% for group, commits in commits | group_by(attribute="group") %}
### {{ group | upper_first }}

    {% for commit in commits %}
        {%- if commit.scope -%}
            - *({{commit.scope}})* {% if commit.breaking %}[**breaking**] {% endif %}\
# highlight-next-line
                {{ commit.message }}{{ self::username(commit=commit) }}\
                {%- if commit.links %} \
                    ({% for link in commit.links %}[{{link.text}}]({{link.href}}) {% endfor -%})\
                {% endif %}
        {% else -%}
# highlight-next-line
            - {% if commit.breaking %}[**breaking**] {% endif %}{{ commit.message }}{{ self::username(commit=commit) }}{{ self::pr(commit=commit) }}
        {% endif -%}
    {% endfor -%}
{% endfor %}
# highlight-start
{%- if remote.contributors %}
### Contributors
{% for contributor in remote.contributors %}
    * @{{ contributor.username }}
{%- endfor %}
{% endif -%}
{%- macro username(commit) -%}
    {% if commit.remote.username %} (by @{{ commit.remote.username }}){% endif -%}
{% endmacro -%}
{%- macro pr(commit) -%}
    {% if commit.remote.pr_number %} - #{{ commit.remote.pr_number }}{% endif -%}
{% endmacro -%}
# highlight-end
"""
```

</details>

<details>
  <summary>Raw Output</summary>

```md
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.1](https://github.com/orhun/git-cliff-readme-example/compare/v1.0.0...v1.0.1) - 2021-07-18

### Added

- Add release script (by @orhun)

### Changed

- Expose string functions (by @orhun)

### Contributors

* @orhun

## [1.0.0] - 2021-07-18

### Added

- Add README.md (by @orhun)
- Add ability to parse arrays (by @orhun)
- Add tested usage example (by @orhun)

### Fixed

- Rename help argument due to conflict (by @orhun)

### Contributors

* @orhun
```

</details>

<details>
  <summary>Rendered Output</summary>

```mdx-code-block
# Changelog
All notable changes to this project will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
## [Unreleased]
## [1.0.1](https://github.com/orhun/git-cliff-readme-example/compare/v1.0.0...v1.0.1) - 2021-07-18
### Added
- Add release script (by @orhun)
### Changed
- Expose string functions (by @orhun)
### Contributors
* @orhun
## [1.0.0] - 2021-07-18
### Added
- Add README.md (by @orhun)
- Add ability to parse arrays (by @orhun)
- Add tested usage example (by @orhun)
### Fixed
- Rename help argument due to conflict (by @orhun)
### Contributors
* @orhun
```

</details>
