# Advanced Configuration

## Git submodules

If your repository uses git submodules, set the `submodules` option in the `actions/checkout` step:

- `submodules: true` to checkout submodules.
- `submodules: recursive` to recursively checkout submodules.

For example:

```yaml
steps:
  - name: Checkout repository
    uses: actions/checkout@v5
    with:
      fetch-depth: 0
# highlight-next-line
      submodules: recursive
```

To learn more, see GitHub [docs](https://github.com/actions/checkout/).

## Add more info to commit message

By default, the commit message of the release PR only contains `chore: release`.
To add the PR title and description to the default commit message when merging a pull request,
change the GitHub repository settings under "General":

![pr settings](../assets/pr_settings.png)

You can learn more in the
[announcement](https://github.blog/changelog/2022-08-23-new-options-for-controlling-the-default-commit-message-when-merging-a-pull-request/)
and
[docs](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/configuring-commit-squashing-for-pull-requests).

## Add additional checks before releasing

To release your crates, Release-plz runs `cargo publish`, which checks if your code
compile before publishing to the cargo registry.

If you want to run other checks before releasing (e.g. `cargo test`), you have two options:

1. *(preferred)* Add the checks in other GitHub actions and run them in the Pull Requests.
   Only merge a PR if the checks are successful.
   The pro of this approach, is that release-plz and your checks run in parallel.
2. Add the checks to the GitHub action before running release-plz:

   ```yml
   jobs:
     release-plz:
       name: Release-plz release
       runs-on: ubuntu-latest
       concurrency:
         group: release-plz-${{ github.ref }}
         cancel-in-progress: false
       steps:
         - name: Checkout repository
           uses: actions/checkout@v5
           with:
             fetch-depth: 0
         - name: Install Rust toolchain
           uses: dtolnay/rust-toolchain@stable
   # highlight-next-line
         - run: cargo test # <-- put any check you like here
         - name: Run release-plz
           uses: release-plz/action@v0.5
           with:
             command: release
           env:
             GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
             CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
   ```

   The con of this approach is that the release-plz action will complete later
   because it needs to wait for the other checks to finish.

## Close old release PRs

Release-plz updates the release PR by force-pushing to it.
If you want release-plz to open new release PRs instead of updating the old ones,
you can close the old release PR before running release-plz:

```yml
jobs:
  release-plz:
    name: Release-plz
    runs-on: ubuntu-latest
    concurrency:
      group: release-plz-${{ github.ref }}
      cancel-in-progress: false
    steps:
      - name: Checkout repository
        uses: actions/checkout@v5
        with:
          fetch-depth: 0
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Close old release PR
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          # List all opened PRs which head branch starts with "release-plz-"
          release_pr=$(gh pr list --state='open' --json number,headRefName --jq '.[] | select(.headRefName | startswith("release-plz-")) | .number')
          # Close the release PR if there is one
          if [[ -n "$release_pr" ]]; then
            echo "Closing old release PR $release_pr"
            gh pr close $release_pr
          else
            echo "No open release PR"
          fi
      - name: Run release-plz PR
        uses: release-plz/action@v0.5
        with:
          command: release-pr
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
```

## Run on schedule

The [quickstart](./quickstart.md) guide configures release-plz to run every time you merge a
commit to the `main` branch.

To run release-plz periodically, you can use the
[`schedule`](https://docs.github.com/en/actions/reference/events-that-trigger-workflows#schedule) event:

```yaml
# Trigger the workflow every Monday.
on:
  schedule:
    # * is a special character in YAML so you have to quote this string
    - cron:  '0 0 * * MON'
```
