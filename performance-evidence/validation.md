# Combined production validation

All six production changes were applied together to main at `8d3c2269` and
reviewed independently. Validation passed with no benchmark overlays in this tree:

```sh
cargo fmt --all -- --check
cargo test --locked --target-dir /workspace/release-plz/target \
  --no-default-features --features all-static --workspace
cargo clippy --locked --target-dir /workspace/release-plz/target \
  --all-targets --all-features --workspace -- -D warnings
git diff --check
```

The test run passed **346 tests**, with **83 ignored**: 80 Docker-dependent
integration tests were disabled and three existing core tests were ignored.
Formatting, Clippy with warnings denied, and whitespace checks passed.

An initial test run exported `CARGO_TARGET_DIR` for the whole process tree.
One existing packaging fixture expects its archive under its own `target`
directory, so that setup redirected its output and made it fail. Removing the
environment export and using Cargo's `--target-dir` argument fixed the fixture;
the entire suite was rerun successfully without changing any test or production
code. Both logs are retained in `validation/`.

The validated production patches correspond to these independent commits:

| Change | Commit |
| --- | --- |
| Default 0.0.x version increments | `1f833e83` |
| Lazy Git changelog defaults | `8723398a` |
| Release-only changelog header matching | `d8875016` |
| Workspace member lookup | `46ddf488` |
| One dependency-table buffer | `c152f165` |
| Workspace version inheritance discovery | `45c9bdee` |

Each production PR targets `main` and contains one of these changes, with no
benchmark code, Cargo dependency changes, or dependency on another production PR.
