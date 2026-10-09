
#[cfg(test)]
mod perf_bench_release_order {
    use super::*;
    use std::{hint::black_box, time::Instant};
    use fake_package::{FakeDependency, FakePackage};

    #[test]
    #[ignore]
    fn benchmark_release_order() {
        for (size, iterations) in [(10_usize, 100_000), (100, 10_000), (1000, 500)] {
            for (name, fanout) in [("independent", 0), ("chain", 1), ("shared_dependencies", 4)] {
                let owned: Vec<Package> = (0..size).map(|index| {
                    let deps = (index.saturating_sub(fanout)..index)
                        .map(|dependency| FakeDependency::new(format!("package-{dependency:04}")))
                        .collect();
                    FakePackage::new(format!("package-{index:04}"))
                        .with_dependencies(deps).into()
                }).collect();
                // Reverse order exercises DFS rather than an already-sorted input.
                let packages: Vec<_> = owned.iter().rev().collect();
                let expected: Vec<_> = if fanout == 0 {
                    packages.iter().map(|p| p.name.as_str()).collect()
                } else {
                    owned.iter().map(|p| p.name.as_str()).collect()
                };
                for _ in 0..50 { black_box(release_order(black_box(&packages)).unwrap()); }
                let start = Instant::now();
                for _ in 0..iterations {
                    black_box(release_order(black_box(&packages)).unwrap());
                }
                let nanos = start.elapsed().as_nanos() as f64 / iterations as f64;
                let actual = release_order(&packages).unwrap();
                assert_eq!(actual.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), expected);
                println!("RELEASE_ORDER scenario={name} packages={size} iterations={iterations} ns_per_call={nanos:.3}");
            }
        }
    }
}
