# Persist credentials

:::info
Read this section if you configured git to sign tags, i.e. if
`git config --get tag.gpgSign` returns `true`.
:::

Normally, release-plz uses the GitHub API to push commits, branches, and tags.
This allows to set `persist-credentials: false` in the
[actions/checkout](https://github.com/actions/checkout?tab=readme-ov-file#usage)
step, which is a good security practice (the default is `true`).

However, the GitHub API doesn't support signed tags, so if release-plz
detects that you sign tags, it will use the git CLI to push the signed tags.

For this reason, you must set `persist-credentials: true` in the
`actions/checkout` step of the job that runs `release-plz release`.

Another reason to set `persist-credentials: true` is you run additional steps
after the release-plz jobs that use the git CLI to push changes to the repository (e.g., update files in a release PR)

:::tip
For more information on the security implications of `persist-credentials`,
see the [zizmor documentation](https://docs.zizmor.sh/audits/#artipacked).
:::
