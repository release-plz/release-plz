# Format

By default, release-plz uses the
[keep a changelog](https://keepachangelog.com/en/1.1.0/) format.

You can customize the changelog format in the [`[changelog]`](../config.md#the-changelog-section)
section of the configuration.

## How should I write my commits?

Release-plz assumes you are using [Conventional Commit messages](https://www.conventionalcommits.org/).

The most important prefixes you should have in mind are:

- `fix:`: represents bug fixes, and results in a [SemVer](https://semver.org/)
  patch bump.
- `feat:`: represents a new feature, and results in a SemVer minor bump.
- `<prefix>!:` (e.g. `feat!:`): represents a breaking change
  (indicated by the `!`) and results in a SemVer major bump.

Commits that don't follow the Conventional Commit format result in a SemVer patch bump.

## Body template

A template is a text where variables and expressions get replaced with values when it is rendered.
By providing a custom [`body`](../config.md#the-body-field) template, you can customize the
changelog format.

### Syntax

**git-cliff** uses the [Tera](https://keats.github.io/tera/) template engine.

See the [Tera Documentation](https://keats.github.io/tera/docs/#templates) for more information
about [control structures](https://keats.github.io/tera/docs/#control-structures),
[built-in filters](https://keats.github.io/tera/docs/#built-ins), etc.

Custom built-in filters that **git-cliff** uses:

- `upper_first`: Converts the first character of a string to uppercase.

### Context

The context contains the data used to render the template.
In the following, we represent the context used for the
changelog generation using [JSON](https://en.wikipedia.org/wiki/JSON).

For a conventional commit like:

```text
<type>[scope]: <description>

[body]

[footer(s)]
```

You can use the following context in the template:

```json
{
  "version": "0.1.1",
  "package": "my_crate",
  "release_link": "https://github.com/me/my_project/compare/my_project-v0.1.0...my_project-v0.1.1",
  "remote": {
    "owner": "<repo owner>",
    "repo": "<repo name>",
    "link": "<repo link>"
    "contributors": [
      {
        "username": "<GitHub/Gitea/GitLab handle>",
      }
    ]
  },
  "commits": [
    {
      "id": "e795460c9bb7275294d1fa53a9d73258fb51eb10",
      "group": "<type> (overridden by commit_parsers)",
      "scope": "[scope]",
      "message": "<description>",
      "raw_message": "<type>[scope]: <description>\n[body]\n[footer(s)]",
      "body": "[body]",
      "footers": [
        {
          "token": "<name of the footer, such as 'Signed-off-by'>",
          "separator": "<the separator between the token and value, such as ':'>",
          "value": "<the value following the separator",
          "breaking": false
        }
      ],
      "breaking_description": "<description>",
      "breaking": false,
      "conventional": true,
      "merge_commit": false,
      "links": [
        { "text": "(set by link_parsers)", "href": "(set by link_parsers)" }
      ],
      "author": {
        "name": "User Name",
        "email": "user.email@example.com",
      },
      "committer": {
        "name": "User Name",
        "email": "user.email@example.com",
      },
      "remote": {
        "username": "<GitHub/Gitea/GitLab handle>",
        "pr_number": "<Number of the PR/MR associated with the commit>",
      }
    }
  ],
  "commit_id": "a440c6eb26404be4877b7e3ad592bfaa5d4eb210 (release commit)",
  "timestamp": 1625169301,
  "previous": {
    "version": "0.1.0"
  }
}
```

#### Footers

A conventional commit's body may end with any number of structured key-value pairs known as
[footers](https://www.conventionalcommits.org/en/v1.0.0/#specification).

They follow a format similar to [the git trailers convention](https://git-scm.com/docs/git-interpret-trailers):

```text
<token><separator><value>
```

You can access the footers in the template using the `commit.footers` array.
Each object in the array has the following fields:

- `token`, the name of the footer (preceding the separator character)
- `separator`, the footer's separator string (either `: ` or ` #`) <!-- markdownlint-disable MD038 -->
- `value`, the value following the separator character
- `breaking`, which is `true` if this is a `BREAKING CHANGE:` footer, and `false` otherwise

Here are some examples of footers:

- `Signed-off-by: User Name <user.email@example.com>`
- `Reviewed-by: User Name <user.email@example.com>`
- `Fixes #1234`
- `BREAKING CHANGE: breaking change description`

### Breaking Changes

The `breaking` flag is set to `true` when:

- The commit has an exclamation mark after the commit type and scope, e.g.:

  ```text
  feat(scope)!: this is a breaking change
  ```

- Or when the `BREAKING CHANGE:` footer is present:

  ```text
  feat: add xyz

  BREAKING CHANGE: this is a breaking change
  ```

`breaking_description` contains:

- The description of the `BREAKING CHANGE` footer (if present).
- the commit `message` otherwise.

If the `BREAKING CHANGE:` footer is present, the footer is present in `commit.footers`.

See also the [protect_breaking_commits](../config.md#the-protect_breaking_commits-field) field.

### `committer` vs `author`

From the [Git docs](https://git-scm.com/book/en/v2/Git-Basics-Viewing-the-Commit-History):

> You may be wondering what the difference is between author and committer.
> The author is the person who originally wrote the work, whereas the committer is the person who
> last applied the work.
> So, if you send in a patch to a project and one of the core members applies the patch,
> both of you get credit — you as the author, and the core member as the committer.
