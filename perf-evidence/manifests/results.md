
dependency-scan

| Case | Before µs (runs 1 / 2 / 3) | After µs (runs 1 / 2 / 3) | Mean before → after | Reduction |
|---|---|---|---|---|
| dependency_scan/one_matching/1 | 7.040 / 7.036 / 7.298 | 7.023 / 7.021 / 7.003 | 7.125 → 7.016 | 1.5% |
| dependency_scan/no_matching/1 | 6.811 / 6.831 / 6.854 | 0.011 / 0.011 / 0.011 | 6.832 → 0.011 | 99.8% |
| dependency_scan/no_dependencies/1 | 0.009 / 0.010 / 0.010 | 0.009 / 0.013 / 0.009 | 0.010 → 0.010 | -6.9% |
| dependency_scan/registry_only_100/1 | 0.025 / 0.025 / 0.023 | 0.025 / 0.027 / 0.025 | 0.024 → 0.026 | -5.5% |
| dependency_scan/one_matching/10 | 25.477 / 25.742 / 25.615 | 7.213 / 7.011 / 7.082 | 25.611 → 7.102 | 72.3% |
| dependency_scan/no_matching/10 | 26.141 / 25.298 / 25.128 | 0.024 / 0.022 / 0.022 | 25.522 → 0.023 | 99.9% |
| dependency_scan/no_dependencies/10 | 0.009 / 0.010 / 0.009 | 0.010 / 0.009 / 0.009 | 0.009 → 0.009 | 0.0% |
| dependency_scan/registry_only_100/10 | 0.023 / 0.023 / 0.024 | 0.027 / 0.027 / 0.027 | 0.023 → 0.027 | -15.7% |
| dependency_scan/one_matching/100 | 207.272 / 205.940 / 203.563 | 7.178 / 7.198 / 7.194 | 205.592 → 7.190 | 96.5% |
| dependency_scan/no_matching/100 | 209.004 / 202.466 / 202.807 | 0.100 / 0.097 / 0.097 | 204.759 → 0.098 | 100.0% |
| dependency_scan/no_dependencies/100 | 0.009 / 0.009 / 0.009 | 0.009 / 0.009 / 0.009 | 0.009 → 0.009 | 0.0% |
| dependency_scan/registry_only_100/100 | 0.024 / 0.026 / 0.025 | 0.025 / 0.027 / 0.027 | 0.025 → 0.026 | -5.3% |

borrow-updates

| Case | Before µs (runs 1 / 2 / 3) | After µs (runs 1 / 2 / 3) | Mean before → after | Reduction |
|---|---|---|---|---|
| update_manifests/10_packages/0_changelog_bytes | 117.106 / 114.239 / 112.286 | 110.373 / 107.284 / 106.833 | 114.544 → 108.163 | 5.6% |
| update_manifests/10_packages/65536_changelog_bytes | 163.197 / 163.064 / 162.488 | 107.182 / 106.215 / 106.350 | 162.916 → 106.582 | 34.6% |
| update_manifests/10_packages/524288_changelog_bytes | 834.321 / 803.879 / 800.595 | 109.338 / 109.168 / 111.299 | 812.932 → 109.935 | 86.5% |
| update_manifests/100_packages/0_changelog_bytes | 1513.190 / 1526.820 / 1517.650 | 1513.596 / 1512.695 / 1501.719 | 1519.220 → 1509.337 | 0.7% |
| update_manifests/100_packages/65536_changelog_bytes | 2522.905 / 2481.800 / 2459.974 | 1480.600 / 1461.992 / 1490.069 | 2488.226 → 1477.554 | 40.6% |
| update_manifests/100_packages/524288_changelog_bytes | 11474.952 / 11518.416 / 11701.953 | 1486.959 / 1474.481 / 1469.538 | 11565.107 → 1476.993 | 87.2% |
