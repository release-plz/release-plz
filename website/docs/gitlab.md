# GitLab CI

`release-plz` can also run in GitLab CI/CD, however the setup is slightly more complex
than with the Github CI.

## 1. Generate a token for release-plz

`release-plz` authenticates with Gitlab via a token. This needs to be generated and added
to the CI/CD variables:

1. Go to the group or repository where you want `release-plz` to work.
2. Go to Settings -> Access token -> Add new token
    - If you don't see the Access token menu, then you don't have sufficient permissions
      to create tokens
3. Give the token the Maintainer role and grant the `api`, `read_api`, `read_repository`,
   and `write_repository` scopes
4. Configure this token as a masked variable named `RELEASE_PLZ_TOKEN` in
   Settings -> CI/CD -> Variables -> Add variable
5. Go to Manage -> Members and find the bot account that was created for this token
   and get the username
6. The username can be converted to the email address using this format:
   `$USERNAME@noreply.gitlab.com`
7. Configure this email address as a variable named `RELEASE_PLZ_BOT_EMAIL` in
   Settings -> CI/CD -> Variables -> Add variable

## 2. Include the component in your `.gitlab-ci.yml` file

```yaml
include:
  - component: $CI_SERVER_FQDN/release-plz/components/release-plz@0.1.0
```

You can also configure the component using inputs.

For example to change the stage at which the component runs:

```yaml
include:
  - component: $CI_SERVER_FQDN/release-plz/components/release-plz@0.1.0
    inputs:
      stage: "special_deploy_stage"
```

### Using Git submodules hosted elsewhere

If your repository contains submodules that are hosted on a different forge, for example Github,
some things need to be configured. This is only needed if the submodule is hosted elsewhere, Gitlab
supports cloning submodules from Gitlab repositories.

1. Enable recursive cloning of submodules by setting `git_submodule_recursive` to `true`
2. Configure an access token for the Git host by setting `git_credentials` to an array of strings
   which conform to [the `.git-credentials` storage format](https://git-scm.com/docs/git-credential-store#_storage_format):

```yaml
include:
  - component: $CI_SERVER_FQDN/release-plz/components/release-plz@0.1.0
    inputs:
      git_submodule_recursive: true
      git_credentials: ["https://${GH_USERNAME}:${GH_TOKEN}@github.com"]
```

### Add extra options to `release-plz` commands

```yaml
include:
  - component: $CI_SERVER_FQDN/release-plz/components/release-plz@0.1.0
    inputs:
      release_args: "--dry-run"
      pr_args: "-v --no-changelog"
```
