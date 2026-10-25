# Agent instructions

* A clean codebase is more important than maintaining retrocompatibility of the libraries in this workspace.
* Prefer writing integration tests `crates/release_plz/tests/` vs inventing new test setups in `crates/release_plz_core/`, especially if the resulting LOC would be less.
