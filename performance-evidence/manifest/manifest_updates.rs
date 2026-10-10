use std::time::Duration;

use cargo_metadata::{Package, semver::Version};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use release_plz_core::fs_utils::Utf8TempDir;

/// Isolate manifest processing from Git history and Cargo subprocesses. Updating
/// one independent package still scans every workspace manifest for dependents.
fn manifest_updates(c: &mut Criterion) {
    let mut group = c.benchmark_group("manifest_updates");
    group
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    for count in [10, 100] {
        let mut fixture = None;
        let mut iteration = 0;
        group.bench_with_input(
            BenchmarkId::new("independent", count),
            &count,
            |b, &count| {
                let fixture = fixture.get_or_insert_with(|| Fixture::new(count));
                let packages: Vec<_> = fixture.packages.iter().collect();
                let manifest = fixture.directory.path().join("Cargo.toml");
                let package = fixture.directory.path().join("packages/package-0");
                let versions = [Version::new(1, 0, 1), Version::new(1, 0, 2)];
                b.iter(|| {
                    // Alternate versions so every iteration changes the target;
                    // unrelated manifests have no dependency requirements to update.
                    release_plz_core::set_version(
                        &packages,
                        &package,
                        &versions[iteration % versions.len()],
                        &manifest,
                    )
                    .unwrap();
                    iteration += 1;
                });
                let updated =
                    cargo_utils::LocalManifest::try_new(&package.join("Cargo.toml")).unwrap();
                assert_eq!(
                    updated.data["package"]["version"].as_str().unwrap(),
                    versions[(iteration - 1) % versions.len()].to_string(),
                );
            },
        );
    }
    group.finish();
}

struct Fixture {
    directory: Utf8TempDir,
    packages: Vec<Package>,
}

impl Fixture {
    fn new(count: usize) -> Self {
        let directory = Utf8TempDir::new().unwrap();
        let manifest = directory.path().join("Cargo.toml");
        fs_err::write(
            &manifest,
            "[workspace]\nmembers = [\"packages/*\"]\nresolver = \"3\"\n",
        )
        .unwrap();
        for index in 0..count {
            let package = directory.path().join(format!("packages/package-{index}"));
            fs_err::create_dir_all(package.join("src")).unwrap();
            fs_err::write(
                package.join("Cargo.toml"),
                format!(
                    "[package]\nname = \"package-{index}\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
                ),
            )
            .unwrap();
            fs_err::write(package.join("src/lib.rs"), "").unwrap();
        }
        let metadata = cargo_utils::get_manifest_metadata(&manifest).unwrap();
        let packages = cargo_utils::workspace_members(&metadata).unwrap().collect();
        Self {
            directory,
            packages,
        }
    }
}

/// Extend PR #3146 with a workspace where the root and a consumer have many
/// versioned local dependencies, only one of which changes per iteration.
fn dense_manifest_updates(c: &mut Criterion) {
    let mut group = c.benchmark_group("manifest_updates");
    group
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(30);
    for count in [10, 100] {
        let fixture = Fixture::new(count);
        let manifest = fixture.directory.path().join("Cargo.toml");
        let mut root = fs_err::read_to_string(&manifest).unwrap();
        root.push_str("\n[workspace.dependencies]\n");
        for index in 0..count - 1 {
            root.push_str(&format!(
                "package-{index} = {{ path = \"packages/package-{index}\", version = \"1.0.0\" }}\n"
            ));
        }
        fs_err::write(&manifest, root).unwrap();
        let consumer = fixture
            .directory
            .path()
            .join(format!("packages/package-{}/Cargo.toml", count - 1));
        let mut consumer_contents = fs_err::read_to_string(&consumer).unwrap();
        consumer_contents.push_str("\n[dependencies]\n");
        for index in 0..count - 1 {
            consumer_contents.push_str(&format!(
                "package-{index} = {{ path = \"../package-{index}\", version = \"1.0.0\" }}\n"
            ));
        }
        fs_err::write(&consumer, consumer_contents).unwrap();
        let metadata = cargo_utils::get_manifest_metadata(&manifest).unwrap();
        let packages: Vec<_> = cargo_utils::workspace_members(&metadata).unwrap().collect();
        let packages: Vec<_> = packages.iter().collect();
        let package = fixture.directory.path().join("packages/package-0");
        let versions = [Version::new(1, 0, 1), Version::new(1, 0, 2)];
        let mut iteration = 0;
        group.bench_with_input(
            BenchmarkId::new("many_local_dependencies", count),
            &count,
            |b, _| {
                b.iter(|| {
                    release_plz_core::set_version(
                        &packages,
                        &package,
                        &versions[iteration % versions.len()],
                        &manifest,
                    )
                    .unwrap();
                    iteration += 1;
                });
            },
        );
        for path in [manifest, consumer] {
            let updated = cargo_utils::LocalManifest::try_new(&path).unwrap();
            let dependencies = updated.get_dependency_tables().flat_map(|t| t.iter());
            let mut checked = false;
            for (name, dependency) in dependencies {
                let expected = if name == "package-0" {
                    checked = true;
                    versions[(iteration - 1) % versions.len()].to_string()
                } else {
                    "1.0.0".to_owned()
                };
                assert_eq!(dependency["version"].as_str().unwrap(), expected);
            }
            assert!(checked);
        }
    }
    group.finish();
}

criterion_group!(benches, manifest_updates, dense_manifest_updates);
criterion_main!(benches);
