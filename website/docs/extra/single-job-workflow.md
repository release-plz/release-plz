# Single-job workflow

The [quickstart](../github/quickstart.md) splits release-plz into two GitHub
Actions jobs: one for `release` and one for `release-pr`.

As explained in [Input](../github/input.md) docs,
if you want to run both `release` and `release-pr` in sequence in a single job,
omit the `with: command:` field:

```yaml
name: Release-plz

permissions:
  pull-requests: write
  contents: write

on:
  push:
    branches:
      - main

jobs:

  release-plz:
    name: Release-plz
    runs-on: ubuntu-latest
    steps:
      - name: Checkout repository
        uses: actions/checkout@v6
        with:
          fetch-depth: 0
          persist-credentials: false
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Run release-plz
        uses: release-plz/action@v0.5
        # No `command:` line — both `release` and `release-pr` run in order.
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
```

:::warning
This setup is not recommended. Bug reports that might be caused by this setup
(instead of splitting the jobs) won't be investigated.
:::

## Trade-off: concurrency

The two-jobs setup gives `release-pr` its own `concurrency` group.
Combining the two into one job means **either**:

- you don't set a `concurrency` group, and every push runs release-plz.
  So in busy repositories, release-plz runs in parallel.
- you set one `concurrency` group with `cancel-in-progress: false`, which
  serializes runs, and could lead to release-plz releasing the wrong version
  of your packages (because if you merge multiple PRs at once, release-plz
  could be skipped in the commit where you merge the Release PR).
