# Single-job workflow

The [quickstart](../github/quickstart.md) splits release-plz into two GitHub
Actions jobs: one for `release` and one for `release-pr`. There's a good
reason for that — they need different `concurrency` settings, see
[Workflow explanation](../github/quickstart.md#concurrency) — but it does
mean release-plz checks out and installs Rust twice on every run.

If you'd rather pay for one job, omit the `with: command:` field and the
action runs both `release` and `release-pr` in sequence:

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

## Trade-off: concurrency

The split-job setup gives `release-pr` its own `concurrency` group so a
back-to-back push cancels the in-flight PR job (cheap to redo) but never
cancels the `release` job (must not be skipped). Combining the two into one
job means **either**:

- you don't set a `concurrency` group, and a burst of pushes spawns multiple
  full release-plz runs in parallel (fine for small repos, wasteful for big
  ones), **or**
- you set one `concurrency` group with `cancel-in-progress: false`, which
  serializes runs and avoids the wasted parallel work, but doesn't get the
  "cancel the in-flight PR" benefit of the split version.

Pick this single-job shape when you'd rather save the duplicate
checkout / toolchain install than have separate concurrency rules for the
two commands. Stick with the split shape from the quickstart if you have
frequent pushes and want the PR job to short-circuit cleanly.
