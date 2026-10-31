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

criterion_group!(benches, manifest_updates);
criterion_main!(benches);
