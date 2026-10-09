#[cfg(test)]
mod perf_dependency_scan {
    use super::*;
    use crate::test_utils::write_package;
    use std::{hint::black_box, time::Instant};

    // This harness is deliberately external to the production PR. The manifest
    // fixture follows PR #3146, adding one renamed path dependency and controlling
    // how many of the other updated packages it can actually depend on.
    #[test]
    #[ignore]
    fn benchmark_dependency_scan() {
        for count in [1, 10, 100] {
            let directory = crate::fs_utils::Utf8TempDir::new().unwrap();
            let root = directory.path();
            fs_err::write(
                root.join("Cargo.toml"),
                "[workspace]\nmembers = [\"packages/*\"]\nresolver = \"3\"\n",
            )
            .unwrap();
            for index in 0..=count {
                write_package(
                    &root.join(format!("packages/package-{index}")),
                    &format!("package-{index}"),
                    "1.0.0",
                    "",
                );
            }
            write_package(
                &root.join("packages/consumer"),
                "consumer",
                "1.0.0",
                "[dependencies]\nrenamed = { package = \"package-0\", path = \"../package-0\", version = \"1.0\" }\n",
            );
            let metadata = cargo_utils::get_manifest_metadata(&root.join("Cargo.toml")).unwrap();
            let consumer = metadata
                .packages
                .iter()
                .find(|package| package.name == "consumer")
                .unwrap();
            let no_dependencies = metadata
                .packages
                .iter()
                .find(|package| package.name == "package-0")
                .unwrap();
            let mut registry_only = no_dependencies.clone();
            registry_only.dependencies = (0..100)
                .map(|index| fake_package::FakeDependency::new(format!("registry-{index}")).into())
                .collect();
            let packages: Vec<_> = (0..=count)
                .map(|index| {
                    metadata
                        .packages
                        .iter()
                        .find(|package| package.name == format!("package-{index}"))
                        .unwrap()
                })
                .collect();
            for (case, matching, tested) in [
                ("one_matching", true, consumer),
                ("no_matching", false, consumer),
                ("no_dependencies", false, no_dependencies),
                ("registry_only_100", false, &registry_only),
            ] {
                let start_index = usize::from(!matching);
                let updated: Vec<_> = packages[start_index..start_index + count]
                    .iter()
                    .map(|package| (*package, Version::new(1, 1, 0)))
                    .collect();
                let run = || {
                    black_box(tested)
                        .dependencies_to_update(black_box(&updated), None, root, false)
                        .unwrap()
                };
                let expected: Vec<_> = if matching {
                    vec!["package-0"]
                } else {
                    Vec::new()
                };
                assert_eq!(
                    run()
                        .iter()
                        .map(|package| package.name.as_str())
                        .collect::<Vec<_>>(),
                    expected
                );
                for _ in 0..100 {
                    black_box(run());
                }
                let iterations = if case == "no_dependencies" || case == "registry_only_100" {
                    1_000_000
                } else {
                    10_000
                };
                let start = Instant::now();
                for _ in 0..iterations {
                    black_box(run());
                }
                let elapsed = start.elapsed();
                assert_eq!(
                    run()
                        .iter()
                        .map(|package| package.name.as_str())
                        .collect::<Vec<_>>(),
                    expected
                );
                println!(
                    "dependency_scan/{case}/{count}: {:.3} us/iteration ({iterations} iterations)",
                    elapsed.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
                );
            }
        }
    }
}
