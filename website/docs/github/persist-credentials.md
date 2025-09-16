# Persist credentials

Normally, release-plz uses the GitHub API to push commits, branches, and tags.
This allows to set `persist-credentials: false` in the
[actions/checkout](https://github.com/actions/checkout?tab=readme-ov-file#usage)
step, which is a good security practice (the default is `true`).

However, you need to set `persist-credentials: true` in the following cases:

- *signed tags*: the GitHub API doesn't support signed tags, so if release-plz
  detects that you sign tags (i.e. if `git config --get tag.gpgSign` returns `true`),
  it will use the git CLI to push the signed tags.
  For this reason, you must set `persist-credentials: true` in the
  `actions/checkout` step of the job that runs `release-plz release`.
- *git push*: after the release-plz step, you run additional steps that use the git CLI to
  push changes (e.g., update files in the release PR).

:::tip
For more information on the security implications of `persist-credentials`,
see the [zizmor documentation](https://docs.zizmor.sh/audits/#artipacked).
:::
