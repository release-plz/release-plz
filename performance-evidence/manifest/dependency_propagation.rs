use std::hint::black_box;
use std::time::Duration;

use cargo_metadata::semver::Version;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use release_plz_core::fs_utils::Utf8TempDir;
use release_plz_core::package_dependencies::PackageDependencies as _;

/// Isolate dependency propagation from Git history and Cargo subprocesses.
/// A consumer declares many renamed local dependencies and every provider has
/// a new version. Half use workspace inheritance; half use target dependencies.
fn dependency_propagation(c: &mut Criterion) {
    let mut group = c.benchmark_group("dependency_propagation");
    group
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(30);
    for count in [10, 100] {
        let directory = Utf8TempDir::new().unwrap();
        let root = directory.path();
        let manifest = root.join("Cargo.toml");
        let mut workspace =
            "[workspace]\nmembers = [\"packages/*\"]\nresolver = \"3\"\n[workspace.dependencies]\n"
                .to_owned();
        let mut consumer_deps = "[dependencies]\n".to_owned();
        for index in 0..count {
            let name = format!("provider-{index}");
            let package = root.join("packages").join(&name);
            fs_err::create_dir_all(package.join("src")).unwrap();
            fs_err::write(package.join("src/lib.rs"), "").unwrap();
            fs_err::write(
                package.join("Cargo.toml"),
                format!("[package]\nname = \"{name}\"\nversion = \"1.0.0\"\nedition = \"2024\"\n"),
            )
            .unwrap();
            workspace.push_str(&format!("alias-{index} = {{ package = \"{name}\", path = \"packages/{name}\", version = \"=1.0.0\" }}\n"));
            if index < count / 2 {
                consumer_deps.push_str(&format!("alias-{index}.workspace = true\n"));
            }
        }
        consumer_deps.push_str("\n[target.'cfg(unix)'.build-dependencies]\n");
        for index in count / 2..count {
            consumer_deps.push_str(&format!("alias-{index} = {{ package = \"provider-{index}\", path = \"../provider-{index}\", version = \"=1.0.0\" }}\n"));
        }
        fs_err::write(&manifest, workspace).unwrap();
        let consumer = root.join("packages/consumer");
        fs_err::create_dir_all(consumer.join("src")).unwrap();
        fs_err::write(consumer.join("src/lib.rs"), "").unwrap();
        fs_err::write(consumer.join("Cargo.toml"), format!("[package]\nname = \"consumer\"\nversion = \"1.0.0\"\nedition = \"2024\"\n{consumer_deps}")).unwrap();
        let metadata = cargo_utils::get_manifest_metadata(&manifest).unwrap();
        let packages: Vec<_> = cargo_utils::workspace_members(&metadata).unwrap().collect();
        let consumer = packages.iter().find(|p| p.name == "consumer").unwrap();
        let updated: Vec<_> = packages
            .iter()
            .filter(|p| p.name != "consumer")
            .map(|p| (p, Version::new(1, 0, 1)))
            .collect();
        let workspace = cargo_utils::LocalManifest::try_new(&manifest).unwrap();
        let workspace_dependencies = workspace.get_workspace_dependency_table();
        let check = consumer
            .dependencies_to_update(&updated, workspace_dependencies, root, false)
            .unwrap();
        assert_eq!(check.len(), count);
        assert!(
            updated
                .iter()
                .all(|(p, _)| check.iter().any(|found| found.name == p.name))
        );
        group.bench_with_input(
            BenchmarkId::new("many_updated_dependencies", count),
            &count,
            |b, _| {
                b.iter(|| {
                    black_box(
                        consumer
                            .dependencies_to_update(
                                black_box(&updated),
                                workspace_dependencies,
                                root,
                                false,
                            )
                            .unwrap(),
                    )
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, dependency_propagation);
criterion_main!(benches);
