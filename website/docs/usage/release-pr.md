# release-pr

The `release-plz release-pr` command runs [`release-plz update`](update.md) and
opens a GitHub Pull Request that prepares the next release.

When the project maintainer merges the release pull request, the packages are
ready to be published.

Here's an example of a [PR](https://github.com/release-plz/release-plz/pull/377)
opened by release-plz in the release-plz GitHub project itself.

In the following example, I run `release-plz` on the `release-plz` project
itself.
`Release-plz` opens a PR that increases the version and the changelog of the
packages with unpublished changes.

![release-plz release-pr](https://user-images.githubusercontent.com/11428655/160772903-544c7578-7c17-4311-b6ca-a1aefeabe799.gif)

To learn more, run `release-plz release-pr --help`.

## PR update

If there's already an open release PR:

- If the PR contains commits that are not from bots (except the first one),
  release-plz closes the PR to preserve the git history.
  The update mechanism is simple: overwrite everything and force-push. 💥
  Reasoning: changes done by bots are not valuable, so we can overwrite them.
  (Not available on Gitea and GitLab).
- Otherwise, release-plz closes the old PR and opens a new one.
  This is done to preserve the git history of maintainers' changes.
  Release-plz also closes the release PR when it cannot update it
  (for example, the force-push fails due to merge conflicts).

:::info
`release-plz release-pr -p <package>` doesn't open a PR per package.
Instead, release-plz overrides the existing release PR with the changes of the specified package.
:::

## Gitea

`release-plz release-pr` also supports creating PRs for repositories hosted on
Gitea with the `--backend` option:

`release-plz release-pr --git-token <gitea application token> --backend gitea`

The token needs to have the following permissions:

- `read:repository`, `write:repository`: to create the release PR.
- `read:issue`, `write:issue`: to add labels to the release PR.

## Github

On Github, the `release-plz release-pr` will use your `--git-token` to create a commit
through the [GraphQL API](https://docs.github.com/en/graphql) rather
than making a commit locally and pushing the changes.
This allows having a [Verified](https://docs.github.com/en/authentication/managing-commit-signature-verification/about-commit-signature-verification)
commit without specifying a GPG signature.

## GitLab

`release-plz release-pr` also supports creating PRs for repositories hosted on GitLab with
the `--backend gitlab` option.

You need to create a token in your GitLab repo (Settings/Access Tokens) with the following
permissions:

- Role: `Maintainer` or higher
- Scopes:
  - `api` (to read/create/update a release-pr)
  - `write_repository` (to push the release-plz branch)

See the GitLab [project access tokens](https://docs.gitlab.com/ee/user/project/settings/project_access_tokens.html)
docs.

Then you can run `release-plz release-pr` with the following arguments:

`release-plz release-pr --backend gitlab --git-token <gitlab_token>`

## Json output

You can get info about the outcome of this command by appending `-o json` to the command:
Stdout will contain info about the release PR:

```json
{
  "prs": [
    {
      "head_branch": "<head_branch>",
      "base_branch": "<base_branch>",
      "html_url": "<html_url>",
      "number": <pr_number>,
      "releases": [
        {
          "package_name": "<package_name>",
          "version": "<package_version>"
        }
      ]

    }
  ]
}
```

Example:

```json
{
  "prs": [
    {
      "head_branch": "release-plz-2024-04-03T21-57-37Z",
      "base_branch": "main",
      "html_url": "http://localhost:3000/zodpwlgr/xcpayeoa/pulls/1",
      "number": 1,
      "releases": [
        {
          "package_name": "my_package",
          "version": "1.0.3"
        }
      ]
    }
  ]
}
```

- `prs`: An array of objects representing the opened PRs.
  If release-plz didn't open or update a release PR, the `prs` array will be empty.
- `head_branch`: The name of the branch where the changes are implemented.
- `base_branch`: name of the branch the changes are pulled into.
  It is the default branch of the repository. E.g. `main`.

:::info
At the moment, the `release-plz release-pr` command doesn't support opening multiple PRs, but we
plan to add this feature in the future.
:::
