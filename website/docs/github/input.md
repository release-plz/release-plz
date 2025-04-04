# Input variables

The GitHub action accepts the following input variables:

- `command`: The release-plz command to run. Accepted values: `release-pr`,
  `release`. *(By default it runs both commands).*
- `registry`: Registry where the packages are stored.
  The registry name needs to be present in the Cargo config.
  If unspecified, the `publish` field of the package manifest is used.
  If the `publish` field is empty, crates.io is used.
- `manifest_path`: Path to the Cargo.toml of the project you want to update.
  Both Cargo workspaces and single packages are supported.
  *(Defaults to the root directory).*
- `version`: Release-plz version to use. E.g. `0.3.70`. *(Default: latest version).*
- `config`: Release-plz config file location.
  *(Defaults to `release-plz.toml` or `.release-plz.toml`).*
- `token`: Token used to publish to the cargo registry.
  Override the `CARGO_REGISTRY_TOKEN` environment variable, or the `CARGO_REGISTRIES_<NAME>_TOKEN`
  environment variable, used for registry specified in the `registry` input variable.
- `backend`: Forge backend. Valid values: `github`, `gitea`, `gitlab`. *(Defaults to `github`).*
- `verbose`: Print module and source location in logs.
  I.e. adds the `-v` flag to the command. *(Defaults to `false`).*

You can specify the input variables by using the `with` keyword.
For example:

```yaml
steps:
  - ...
  - name: Run release-plz
    uses: release-plz/action@v0.5
# highlight-start
    # Input variables
    with:
      command: release-pr
      registry: my-registry
      manifest_path: rust-crates/my-crate/Cargo.toml
      version: 0.3.70
# highlight-end
    env:
      GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
      CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
```
