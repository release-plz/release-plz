# Performance follow-up evidence

These local-only benchmarks investigate small production changes against
`release-plz/release-plz` main at `8d3c2269`. They extend the workloads in
[benchmark PR #3143](https://github.com/release-plz/release-plz/pull/3143) and
[manifest benchmark PR #3146](https://github.com/release-plz/release-plz/pull/3146).
This evidence branch is independent of the production PRs; none requires merging
the benchmarks or their dependencies.

Each directory contains the workload, reproduction instructions, and recorded
results. Times refer to the operation identified by that benchmark, not an
assumed whole-command speedup. Linux x86_64, five available AMD EPYC 9V45 vCPUs,
Rust 1.99.0, optimized Cargo bench profile. Builds and timed runs used a shared
exclusive lock. No benchmarks ran concurrently with our builds or other timed
runs. Selected noisy controls were repeated, with at most three runs per case.

Rejected experiments are retained where useful: caching dependency paths slowed
unique dependency chains in both run orders, and a lazy mutable-table iterator
was slower for small manifests. Neither belongs in the production changes.
