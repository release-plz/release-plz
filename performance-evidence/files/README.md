# Package file comparison benchmark

This harness calls the actual public `release_plz_core::are_packages_equal`
function. Each fixture has equal manifests and `Cargo.toml.orig` markers on both
sides, making the function compare extracted package directories without
spawning Cargo. Timings include directory listing, manifest comparison, and
payload comparison. These are component measurements, not end-to-end
`release-plz update` timings.

The source payload is deterministic binary content. Each scenario warms the
filesystem cache with ten comparisons, then measures comparisons for 500 ms.
The CSV reports nanoseconds per complete package comparison and the number of
iterations. Run each binary three times, alternating their order. No compilation
or other benchmark should run concurrently with the measurements.

To reproduce, prepare a clean worktree at each recorded commit, then run:

```sh
./run.sh /path/to/baseline-worktree /path/to/changed-worktree /path/to/output
```

The runner builds both versions in release mode, compiles the external harness
against their actual library artifacts, and saves the six CSV files. It touches
the comparison source file's mtime before each build to prevent stale artifacts
when switching worktrees with a shared Cargo target directory. It does not alter
tracked contents or add benchmark files to either production branch. It
requires Rust (Cargo and rustc), Python 3, and the native build dependencies of
release-plz. The output directory must have enough space for Cargo's target
directory.

`equal_100x1k` has 100 source files, each 1 KiB. All other scenarios have one
source file. `first_byte` and `last_byte` change a single byte without changing
the file length. `truncated` removes the last byte. The 8 MiB cases deliberately
exercise large packaged assets; they are not meant to represent a typical Rust
source file.

The reported improvement is `100 * (1 - mean(after) / mean(before))`, using the
arithmetic mean of all three runs. Raw CSV files retain each individual run.
