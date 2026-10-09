When propagating dependency-only releases, each potential dependent currently canonicalizes every updated workspace package and scans its TOML dependencies even when Cargo metadata already proves their names cannot match. Filter updated packages by resolved path-dependency names first, then keep the existing canonical-path check for potential matches. An empty candidate set also avoids opening the manifest.

Cargo metadata resolves renamed, inherited and target-specific dependencies. The existing fast return for packages without path dependencies is preserved.

Benchmark evidence (three before/after runs, release profile, Rust 1.99.0, Linux x86_64; base `267b5775`, change `a13cac01`):

| Workload | Before: runs 1 / 2 / 3 (µs) | After: runs 1 / 2 / 3 (µs) | Mean before → after (µs) | Less time |
|---|---|---|---|---|
| 1 updated package; one matching dependency | 7.040 / 7.036 / 7.298 | 7.023 / 7.021 / 7.003 | 7.125 → 7.016 | 1.5% |
| 10 updated packages; one matching dependency | 25.477 / 25.742 / 25.615 | 7.213 / 7.011 / 7.082 | 25.611 → 7.102 | 72.3% |
| 100 updated packages; one matching dependency | 207.272 / 205.940 / 203.563 | 7.178 / 7.198 / 7.194 | 205.592 → 7.190 | 96.5% |
| 10 updated packages; no matching dependency | 26.141 / 25.298 / 25.128 | 0.024 / 0.022 / 0.022 | 25.522 → 0.023 | 99.9% |
| 100 updated packages; no matching dependency | 209.004 / 202.466 / 202.807 | 0.100 / 0.097 / 0.097 | 204.759 → 0.098 | 99.95% |

These measurements time the actual `Package::dependencies_to_update` phase, including manifest reads/parsing, canonicalization and requirement checks. The temporary harness extends #3146's workspace fixtures with one consumer of a renamed path dependency and 1/10/100 updated packages. Fixture construction and output assertions are outside timing; each normal case runs 10,000 iterations. A single matching update is a control: 7.125 → 7.016 µs, effectively unchanged. Packages without dependencies remain about 9 ns; a synthetic metadata-only control with 100 registry dependencies remains about 25–27 ns even with 100 updates, confirming there is no new scan over all updated packages on that path.

The 100-update no-match mean is 204.759 → 0.098 µs (99.95% less time). These are phase measurements, not a claim about total `release-plz update` runtime. No benchmark code is included in this PR.

Validation: existing `versionless_and_workspace_dependencies_to_update` regression matrix passes. Added a CLI integration regression covering a renamed, inherited, target-specific build dependency among unrelated updated packages.
