# Agent guidance

- A clean codebase is more important than maintaining retrocompatibility of the libraries in this
  workspace.
- Prefer writing integration tests in `crates/release_plz/tests/` over inventing new test setups in
  `crates/release_plz_core/`, especially if the resulting LOC would be less.

Run `rumdl check .` from the repository root after editing Markdown or MDX files.
