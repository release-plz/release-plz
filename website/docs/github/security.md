# Security

In the following, we'll discuss some security considerations when using the release-plz GitHub
action and how to mitigate them.

## Using latest version

The examples provided in the documentation use the latest version of the release-plz GitHub action.

For example, the following snippet uses the `v0.5` version of the release-plz GitHub action:

```yaml
jobs:
  release-plz:
    name: Release-plz
    runs-on: ubuntu-latest
    steps:
      - ...
      - name: Run release-plz
        uses: release-plz/action@v0.5
```

[This](https://github.com/release-plz/action/blob/main/.github/workflows/update_main_version.yml)
script updates this tag to whatever the latest `0.5.x` version is.
This means that if the latest version of release-plz is 0.5.34, with `v0.5` you will use that version.
If tomorrow, release-plz 0.5.35 is released, you will use that version without the
need to update your workflow file.

While this is great for new features and bug fixes, it can also be a security risk.

### ⚠️ Risk: malicious code published on your crates.io crate

An attacker who manages to push and tag malicious code to the GitHub action
[repository](https://github.com/release-plz/action)
could use your cargo registry token to push malicious code to
your crate on crates.io.
This means you or your users could download and run the malicious code.

### ✅ Solution: pin the action version

To mitigate this risk, you can use a specific version of the release-plz GitHub action.
By specifying a commit hash, the action won't be updated automatically.

For example:

```yaml
jobs:
  release-plz:
    name: Release-plz
    runs-on: ubuntu-latest
    steps:
      - ...
      - name: Run release-plz
# highlight-next-line
        uses: release-plz/action@63ab0c2746bedc448370bad4b0b3d536458398b0 # v0.5.50

```

This is the same approach used in the crates.io
[repository](https://github.com/rust-lang/crates.io/blob/7e52e11c5ddeb33db70f0000bbcdfb01e9b43b0d/.github/workflows/ci.yml#L30C32-L31C1).

## Pass environment variables

When using release-plz locally, you might need to pass environment
variables such as `GITHUB_TOKEN` or other sensitive data.

### ⚠️ Risk: the environment variable is stolen

If you pass the environment variable directly in the command line, it can be
stolen by an attacker who gets access to your terminal history.

### ✅ Solution: Use a password manager

Store the token in a password manager and retrieve it when needed.
For example, [here's](https://developer.1password.com/docs/cli/secrets-environment-variables/)
how to do it with 1password.

An alternative is storing the token in a `.env` file and adding this file to the `.gitignore`.
You could `source` this file or use an external tool like [dotenvx](https://github.com/dotenvx/dotenvx), which also supports encryption, so that an
attacker who has access to the `.env` file cannot read the token easily.

## `zizmor` warning

[zizmor](https://github.com/woodruffw/zizmor) is a static analysis tool for GitHub Actions.
When you run it on the release-plz [workflow](./quickstart.md#3-setup-the-workflow), it will
emit the [artipacked](https://woodruffw.github.io/zizmor/audits/#artipacked) warning:

```text
warning[artipacked]: credential persistence through GitHub Actions artifacts
  --> .github/workflows/release-plz.yml:24:9
   |
24 |         - name: Checkout repository
   |  _________-
25 | |         uses: actions/checkout@v4
26 | |         with:
27 | |           fetch-depth: 0
   | |________________________- does not set persist-credentials: false
   |
   = note: audit confidence → Low
```

This warning is emitted because the `actions/checkout` action does not set
`persist-credentials: false` in the `with` section.

Unfortunately, `persist-credentials` needs to be set to `true` (which is the default)
for the release-plz action to work because release-plz needs the token generated
by the `actions/checkout` action to run git commands like `git tag` and `git push`.

To solve the warning, set `persist-credentials: true` in the `with` section
of the `actions/checkout` action:

```yaml
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4
        with:
          fetch-depth: 0
# highlight-start
          persist-credentials: true
# highlight-end
```
