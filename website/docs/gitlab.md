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

## 2. Put the component definition in a repository

The component definition below takes care of a lot of edgecases, and it's therefore recommended to
put this in a separate repository and so that the component can be used by all your Rust repositories.

```yaml
spec:
  inputs:
    stage:
      type: string
      default: "deploy"
      description: "At which stage this component runs"
    image:
      default: "rust"
      description: "The Docker image to run in, needs to have `Cargo` installed"
    cwd:
      type: string
      default: "."
      description: "The directory where to run release-plz, relative to $CI_PROJECT_DIR"
    tags:
      type: array
      default: []
      description: "Tags to apply to this job"
    git_credentials:
      type: array
      default: []
      description: 'Credentials for a Git host in the form of `"https://user:pass@example.com"`, there's a default credential for `$CI_SERVER_HOST` which can be overwritten here'
    git_submodule_recursive:
      type: boolean
      default: false
      description: "Recursively clone submodules before running release-plz"
    bot_email:
      type: string
      default: "$RELEASE_PLZ_BOT_EMAIL"
      description: "The email address associated with `$RELEASE_PLZ_TOKEN`"
    bot_name:
      type: string
      default: "release-plz"
      description: "The name to use as commit author"
    release_args:
      type: string
      default: ""
      description: "Arguments to pass to `release-plz release`"
    pr_args:
      type: string
      default: ""
      description: "Arguments to pass to `release-plz pr`"
---
.release-plz-defaults:
  stage: $[[ inputs.stage ]]
  image: $[[ inputs.image ]]
  tags: $[[ inputs.tags ]]
  variables:
    # Clone the entire repo, this prevents issues with partial checkouts
    GIT_STRATEGY: "clone"
  rules:
    - if: $CI_COMMIT_TAG
      when: never
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
  cache:
    # Cache the crates.io index and downloads
    key: cargo-crates-io
    paths:
      - cargo_registry_index/
      - cargo_registry_cache/
  before_script:
    # Restore the registry cache if it exists
    # The `|| true` is so that the commands can fail if the cache is empty
    - rm -rf "$CARGO_HOME/registry/index/" || true
    - rm -rf "$CARGO_HOME/registry/cache/" || true
    - mv cargo_registry_index/ "$CARGO_HOME/registry/index/" || true
    - mv cargo_registry_cache/ "$CARGO_HOME/registry/cache/" || true
    # Have git get credentials from a file
    - git config --global credential.helper store
    # Convert the `inputs.git_credentials` array into something that sh can understand
    - GIT_CREDENTIALS=$(echo $(eval 'echo "$[[ inputs.git_credentials ]]"') | tr -d '[],"')
    - set -- $GIT_CREDENTIALS
    # This will output an empty line if `inputs.get_credentials` is empty, this is officially not
    # supported but Git accepts it anyway
    - for cred in "$@"; do echo "$cred" >> ~/.git-credentials; done
    # The `$CI_SERVER_HOST` default credential is done last, as Git stops reading at the first match
    # Thus this allows a user to specify another credential for `$CI_SERVER_HOST`
    - echo "$CI_SERVER_PROTOCOL://gitlab-ci-token:${RELEASE_PLZ_TOKEN}@$CI_SERVER_HOST" >> ~/.git-credentials
    # Gitlab doesn't do a proper checkout, also recurse submodules if requested (Gitlab can do it, but not for repo's hosted elsewhere)
    - git checkout $( if [ "$[[ inputs.git_submodule_recursive ]]" = "true" ]; then echo "--recurse-submodules"; fi ) "$CI_COMMIT_BRANCH"
    - if [ $[[ inputs.git_submodule_recursive ]] = "true" ]; then git submodule update --init; fi
    # Check that the branch hasn't updated before the CI got to start
    - if [ "$( git rev-parse HEAD )" != "$CI_COMMIT_SHA" ]; then echo "$CI_COMMIT_BRANCH$ got updated after $CI_COMMIT_SHA, restart the CI on the latest commit"; exit 1; fi
    # release-plz creates a commit, so Git needs an identity
    - git config --global user.email "$[[ inputs.bot_email ]]"
    - git config --global user.name "$[[ inputs.bot_name ]]"
    # release-plz doesn't use git-token for repository authentication, see https://github.com/release-plz/release-plz/issues/2174
    - git remote set-url origin "$CI_SERVER_PROTOCOL://anything:$RELEASE_PLZ_TOKEN@$CI_SERVER_HOST/$CI_PROJECT_PATH.git"
    # Install binstall and release-plz, compiling from source would take too long
    - curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
    - cargo binstall release-plz
    # Change to the user provided directory
    - cd "$[[ inputs.cwd ]]"
  after_script:
    # Update the registry cache
    - mv "$CARGO_HOME/registry/index/" cargo_registry_index/ || true
    - mv "$CARGO_HOME/registry/cache/" cargo_registry_cache/ || true

# Creates/updates a PR to update the version and changelog
release-plz:pr:
  extends: .release-plz-defaults
  script:
    - release-plz release-pr --forge gitlab --git-token "$RELEASE_PLZ_TOKEN" $[[ inputs.pr_args ]]

# This only creates a release if the version in `Cargo.toml` is newer than the version on `crates.io`
release-plz:release:
  extends: .release-plz-defaults
  script:
    - release-plz release --forge gitlab --git-token "$RELEASE_PLZ_TOKEN" $[[ inputs.release_args ]]

```

## 3. Use the component in your repository

By putting it at `templates/release-plz/template.yml` it can be used in your repositories in
your `.gitlab-ci.yml` as:

```yaml
include:
  - component: $CI_SERVER_FQDN/path/to/component/release-plz@branch_name
```

You can also configure the components via the inputs specified in the `spec` section.

For example to change the stage at which the component runs:

```yaml
include:
  - component: $CI_SERVER_FQDN/path/to/component/release-plz@branch_name
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
  - component: $CI_SERVER_FQDN/path/to/component/release-plz@branch_name
    inputs:
      git_submodule_recursive: true
      git_credentials: ["https://${GH_USERNAME}:${GH_TOKEN}@github.com"]
```

### Add extra options to `release-plz` commands

```yaml
include:
  - component: $CI_SERVER_FQDN/path/to/component/release-plz@branch_name
    inputs:
      release_args: "--dry-run"
      pr_args: "-v --no-changelog"
```
