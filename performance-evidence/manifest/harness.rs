#[cfg(test)]
mod performance_benchmarks {
    use super::*;
    use crate::{fs_utils::Utf8TempDir, test_utils::write_package};
    use std::{hint::black_box, time::{Duration, Instant}};

    #[test]
    #[ignore]
    fn benchmark_dependency_propagation() {
        for workspace_count in [50, 500] {
            let directory = Utf8TempDir::new().unwrap();
            let root = directory.path();
            let mut workspace = String::from("[workspace]\nmembers = [\"*\"]\nresolver = \"3\"\n[workspace.dependencies]\n");
            let mut inherited = String::from("[dependencies]\n");
            for index in 0..workspace_count {
                let name = format!("support-{index}");
                write_package(&root.join(&name), &name, "0.1.0", "");
                workspace.push_str(&format!("alias-{index} = {{ package = \"{name}\", path = \"{name}\", version = \"0.1\" }}\n"));
                if index % (workspace_count / 10) == 0 {
                    inherited.push_str(&format!("alias-{index}.workspace = true\n"));
                }
            }
            write_package(&root.join("consumer"), "consumer", "0.1.0", &inherited);
            fs_err::write(root.join("Cargo.toml"), workspace).unwrap();
            let metadata = cargo_utils::get_manifest_metadata(&root.join("Cargo.toml")).unwrap();
            let manifest = LocalManifest::try_new(&root.join("Cargo.toml")).unwrap();
            let consumer = metadata.packages.iter().find(|p| p.name == "consumer").unwrap();
            let support_name = format!("support-{}", workspace_count - workspace_count / 10);
            let support = metadata.packages.iter().find(|p| p.name.as_str() == support_name).unwrap();
            let updated = [(support, Version::new(0, 2, 0))];
            let operation = || {
                let result = consumer.dependencies_to_update(
                    black_box(&updated),
                    manifest.get_workspace_dependency_table(),
                    root,
                    false,
                ).unwrap();
                assert_eq!(result.len(), 1);
                assert_eq!(result[0].name, support.name);
                black_box(result);
            };
            for _ in 0..100 { operation(); }
            let start = Instant::now();
            let mut iterations = 0;
            while start.elapsed() < Duration::from_secs(3) {
                for _ in 0..100 { operation(); }
                iterations += 100;
            }
            let elapsed = start.elapsed();
            println!("inherited_lookup templates={workspace_count} consumer_dependencies=10 iterations={iterations} elapsed_ns={} ns_per_iteration={:.3}", elapsed.as_nanos(), elapsed.as_nanos() as f64 / iterations as f64);
        }

        for consumer_count in [10, 100] {
            let directory = Utf8TempDir::new().unwrap();
            let root = directory.path();
            fs_err::write(root.join("Cargo.toml"), "[workspace]\nmembers = [\"*\"]\nresolver = \"3\"\n").unwrap();
            for index in 0..consumer_count {
                write_package(&root.join(format!("consumer-{index}")), &format!("consumer-{index}"), "0.1.0", "");
            }
            write_package(&root.join("support"), "support", "0.1.0", "");
            let metadata = cargo_utils::get_manifest_metadata(&root.join("Cargo.toml")).unwrap();
            let support = metadata.packages.iter().find(|p| p.name == "support").unwrap();
            let consumers: Vec<_> = metadata.packages.iter().filter(|p| p.name != "support").collect();
            let updated = [(support, Version::new(0, 2, 0))];
            let operation = || {
                for consumer in &consumers {
                    let result = consumer.dependencies_to_update(black_box(&updated), None, root, false).unwrap();
                    assert!(result.is_empty());
                    black_box(result);
                }
            };
            for _ in 0..100 { operation(); }
            let start = Instant::now();
            let mut iterations = 0;
            while start.elapsed() < Duration::from_secs(3) {
                for _ in 0..100 { operation(); }
                iterations += 100;
            }
            let elapsed = start.elapsed();
            println!("independent_dependents consumers={consumer_count} iterations={iterations} elapsed_ns={} ns_per_iteration={:.3}", elapsed.as_nanos(), elapsed.as_nanos() as f64 / iterations as f64);
        }
    }
}
