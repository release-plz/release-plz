# Output

When the action runs with `command: release-pr`, it outputs the following properties:

- `prs`: The release PRs opened by release-plz.
  It's an array of objects with the properties of `pr`.
  *(Not useful for now. Use `pr` instead)*.
- `pr`: The release PR opened by release-plz.
  It's a JSON object with the following properties:
  - `head_branch`: The name of the branch where the changes are implemented.
  - `base_branch`: The name of the branch the changes are pulled into.
    It is the default branch of the repository. E.g. `main`.
  - `html_url`: The URL of the PR.
  - `number`: The number of the PR.
  - `releases`: Array of packages edited in the PR.
    Each entry is an object containing:
    - `package_name`: The name of the edited package.
    - `version`: The next version of the package. The PR updates the package to this version.
- `prs_created`: Whether release-plz created any release PR. *"true" or "false".*

When the action runs with `command: release`, it outputs the following properties:

- `releases`: The JSON output of the `release` command.
  It's an array of JSON objects with the following properties:
  - `package_name`: The name of the package that was released.
  - `prs`: Array of PRs present in the changelog body of the release.
    Usually, they are the PRs containing the changes that were released.
    Each entry is an object containing:
    - `html_url`: The URL of the PR.
    - `number`: The number of the PR.
  - `tag`: git tag name of the package that was released. It's returned even if you have
    [git_tag_enable](../config.md#the-git_tag_enable-field) set to `false`, so that
    you can use this to create the git tag yourself.
  - `version`: The version of the package that was released.
- `releases_created`: Whether release-plz released any package. *"true" or "false".*

## Example: read the output

```yaml
jobs:

  release-plz-release:
    name: Release-plz release
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - &checkout
        name: Checkout repository
        uses: actions/checkout@v5
        with:
          fetch-depth: 0
          persist-credentials: false
      - &install-rust
        name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Run release-plz
# highlight-next-line
        id: release-plz # <--- ID used to refer to the outputs. Don't forget it.
        uses: release-plz/action@v0.5
        with:
          command: release
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
      - name: Read release output
        env:
          RELEASES: ${{ steps.release-plz.outputs.releases }}
          RELEASES_CREATED: ${{ steps.release-plz.outputs.releases_created }}
        run: |
          set -e
          echo "releases: $RELEASES" # example: [{"package_name":"my-package","prs":[{"html_url":"https://github.com/user/proj/pull/1439","number":1439}],"tag":"v0.1.0","version":"0.1.0"}]
          echo "releases_created: $RELEASES_CREATED" # example: true

          # get the number of releases with jq
          releases_length=$(echo "$RELEASES" | jq 'length')
          echo "releases_length: $releases_length"

          # access the first release with jq
          release_version=$(echo "$RELEASES" | jq -r '.[0].version')
          echo "release_version: $release_version"

          # access the first release with fromJSON. Docs: https://docs.github.com/en/actions/learn-github-actions/expressions
          echo "release_version: ${{ fromJSON(steps.release-plz.outputs.releases)[0].version }}"

          release_tag=$(echo "$RELEASES" | jq -r '.[0].tag')
          echo "release_tag: $release_tag"

          release_package_name=$(echo "$RELEASES" | jq -r '.[0].package_name')
          echo "release_package_name: $release_package_name"

          # print all names of released packages, one per line
          echo "package_names: $(echo "$RELEASES" | jq -r '.[].package_name')"
          # TODO: show how to store this in a variable and iterate over it (maybe an array?). PR welcome!

          # iterate over released packages
          for package_name in $(echo "$RELEASES" | jq -r '.[].package_name'); do
              echo "released $package_name"
          done

  release-plz-pr:
    name: Release-plz PR
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
    concurrency:
      group: release-plz-${{ github.ref }}
      cancel-in-progress: false
    steps:
      - *checkout
      - *install-rust
      - name: Run release-plz
# highlight-next-line
        id: release-plz # <--- ID used to refer to the outputs. Don't forget it.
        uses: release-plz/action@v0.5
        with:
          command: release-pr
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
      - name: Read release-pr output
        env:
          PRS: ${{ steps.release-plz.outputs.prs }}
          PR: ${{ steps.release-plz.outputs.pr }}
          PRS_CREATED: ${{ steps.release-plz.outputs.prs_created }}
        run: |
          set -e
          echo "prs: $PRS" # example: [{"base_branch":"main","head_branch":"release-plz-2024-05-01T20-38-05Z","html_url":"https://github.com/MarcoIeni/rust-workspace-example/pull/198","number":198, "releases":[{"package_name":"my-package","version":"0.1.0"}]}]
          echo "pr: $PR" # example: {"base_branch":"main","head_branch":"release-plz-2024-05-01T20-38-05Z","html_url":"https://github.com/MarcoIeni/rust-workspace-example/pull/198","number":198}
          echo "prs_created: $PRS_CREATED" # example: true

          echo "pr_number: ${{ fromJSON(steps.release-plz.outputs.pr).number }}"
          echo "pr_html_url: ${{ fromJSON(steps.release-plz.outputs.pr).html_url }}"
          echo "pr_head_branch: ${{ fromJSON(steps.release-plz.outputs.pr).head_branch }}"
          echo "pr_base_branch: ${{ fromJSON(steps.release-plz.outputs.pr).base_branch }}"
```

## Example: add labels to released PRs

It often happens, when looking for a feature or a bug fix, to land on a merged PR.
The next question: was this released? In what version?

With release-plz you can add a label to the PRs with the version they were released in:

:::info
In this example, we are talking about the PRs containing code changes.
We aren't talking about the release PRs created by release-plz.
You can label release PRs with the [pr_labels](../config.md#the-pr_labels-field)
configuration field.
:::

```yaml
jobs:
  release-plz-pr:
    runs-on: ubuntu-latest
    permissions:
      pull-requests: write
      contents: write
    concurrency:
      group: release-plz-${{ github.ref }}
      cancel-in-progress: false
    steps:
      - name: Checkout repository
        uses: actions/checkout@v5
        with:
          fetch-depth: 0
          persist-credentials: false
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Run release-plz
# highlight-next-line
        id: release-plz # <--- ID used to refer to the outputs. Don't forget it.
        uses: release-plz/action@v0.5
        with:
          command: release
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
      - name: Tag released PRs
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          RELEASES: ${{ steps.release-plz.outputs.releases }}
        run: |
          set -e

          # Iterate over released packages and add a label to the PRs
          # shipped with the release.
          for release in $(echo "$RELEASES" | jq -r -c '.[]'); do
              package_name=$(echo "$release" | jq -r '.package_name')
              version=$(echo "$release" | jq -r '.version')
              prs_length=$(echo "$release" | jq '.prs | length')
              if [ "$prs_length" -gt 0 ]; then
                  # Create label.
                  # Use `--force` to overwrite the label,
                  # so that the command does not fail if the label already exists.
                  label="released:$package_name-$version"
                  echo "Creating label $label"
                  gh label create $label --color BFD4F2 --force
                  for pr in $(echo "$release" | jq -r -c '.prs[]'); do
                      pr_number=$(echo "$pr" | jq -r '.number')
                      echo "Adding label $label to PR #$pr_number"
                      gh pr edit $pr_number --add-label $label
                  done
              else
                  echo "No PRs found for package $package_name"
              fi
          done
```

You can also add a milestone with `gh pr edit $pr_number --milestone <MILESTONE_NUMBER>`.

:::tip
Make sure your GitHub token has permission to do all the operations you need.
:::

## Example: commit files to the release PR

You can commit files to the release PR opened by release-plz.

```yaml
jobs:
  release-plz-pr:
    runs-on: ubuntu-latest
    permissions:
      pull-requests: write
      contents: write
    concurrency:
      group: release-plz-${{ github.ref }}
      cancel-in-progress: false
    steps:
      - name: Checkout repository
        uses: actions/checkout@v5
        with:
          fetch-depth: 0
# highlight-next-line
          persist-credentials: true
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Run release-plz
# highlight-next-line
        id: release-plz # <--- ID used to refer to the outputs. Don't forget it.
        uses: release-plz/action@v0.5
        with:
          command: release-pr
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
      - name: Update README in the release PR
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          PR: ${{ steps.release-plz.outputs.pr }}
        run: |
          set -e

          pr_number=${{ fromJSON(steps.release-plz.outputs.pr).number }}
          if [[ -n "$pr_number" ]]; then
            gh pr checkout $pr_number
            # change "echo" with your commands
            echo "new readme" > README.md
            git add .
            git commit -m "Update README"
            git push
          fi
```

:::info
To learn why we set `persist-credentials: true`, see
[Signed tags](../github/signed-tags.md).
:::
