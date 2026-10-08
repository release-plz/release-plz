| Workload | Before, runs 1 / 2 / 3 (µs) | After, runs 1 / 2 / 3 (µs) | Median time decrease |
| --- | --- | --- | --- |
| 10 independent members | 55.74, 56.07, 74.80 | 13.74, 13.89, 16.26 | 75.2% (4.04× faster) |
| 100 independent members | 439.26, 441.84, 464.60 | 14.66, 14.00, 16.04 | 96.7% (30.14× faster) |

Each value is Criterion’s central time estimate for a complete benchmark run; the decrease compares the median of three runs. Linux x86-64, Intel Xeon Platinum 8573C, rustc 1.99.0, release/bench profile.
