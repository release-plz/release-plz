use std::{hint::black_box, time::Duration};

use cargo_metadata::{Metadata, camino::Utf8Path};
use cargo_utils::{LocalManifest, Manifest};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

fn workspace_members(c: &mut Criterion) {
    let mut group = c.benchmark_group("workspace_members");
    group
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    for (count, shared) in [(10, 3), (100, 10), (100, 0), (100, usize::MAX)] {
        let (_directory, metadata) = workspace_fixture(count, shared);
        let members = cargo_utils::workspace_members(&metadata)
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(members.len(), count);
        for member in &members {
            assert_eq!(
                member.manifest_path,
                dunce::canonicalize(&member.manifest_path)
                    .unwrap()
                    .to_str()
                    .unwrap()
            );
            for dependency in &member.dependencies {
                let path = dependency.path.as_ref().unwrap();
                assert_eq!(
                    path.as_str(),
                    dunce::canonicalize(path).unwrap().to_str().unwrap()
                );
            }
        }
        group.bench_with_input(
            BenchmarkId::new(
                if shared == usize::MAX {
                    "unique_chain".to_owned()
                } else {
                    format!("shared_{shared}")
                },
                count,
            ),
            &metadata,
            |b, metadata| {
                b.iter(|| {
                    black_box(
                        cargo_utils::workspace_members(black_box(metadata))
                            .unwrap()
                            .collect::<Vec<_>>(),
                    )
                });
            },
        );
    }
    group.finish();
}

fn workspace_fixture(count: usize, shared: usize) -> (tempfile::TempDir, Metadata) {
    let directory = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(directory.path()).unwrap();
    fs_err::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"packages/*\"]\nresolver = \"3\"\n",
    )
    .unwrap();
    for i in 0..count {
        let package = root.join(format!("packages/package-{i}"));
        fs_err::create_dir_all(package.join("src")).unwrap();
        let mut manifest = format!(
            "[package]\nname = \"package-{i}\"\nversion = \"1.0.0\"\nedition = \"2024\"\n[dependencies]\n"
        );
        let start = if shared == usize::MAX {
            i.saturating_sub(1)
        } else {
            0
        };
        for j in start..i.min(shared) {
            manifest.push_str(&format!(
                "package-{j} = {{ path = \"../package-{j}\", version = \"1\" }}\n"
            ));
        }
        fs_err::write(package.join("Cargo.toml"), manifest).unwrap();
        fs_err::write(package.join("src/lib.rs"), "").unwrap();
    }
    let metadata = cargo_utils::get_manifest_metadata(&root.join("Cargo.toml")).unwrap();
    (directory, metadata)
}

fn dependency_tables(c: &mut Criterion) {
    let mut group = c.benchmark_group("dependency_tables_mut");
    group
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    for targets in [0, 10, 100] {
        let mut contents = String::from(
            "[package]\nname = 'example'\nversion = '1.0.0'\n[workspace]\nmembers = []\n[workspace.dependencies]\nshared = '1'\n",
        );
        for prefix in std::iter::once(String::new())
            .chain((0..targets).map(|i| format!("target.'cfg(feature = \"target{i}\")'.")))
        {
            for kind in ["dependencies", "build-dependencies", "dev-dependencies"] {
                contents.push_str(&format!("[{prefix}{kind}]\nshared = '1'\n"));
            }
        }
        let mut manifest = LocalManifest {
            path: "/unused/Cargo.toml".into(),
            manifest: contents.parse::<Manifest>().unwrap(),
        };
        assert_eq!(
            manifest.get_dependency_tables_mut().count(),
            4 + targets * 3
        );
        for table in manifest.get_dependency_tables_mut() {
            table.insert("marker", toml_edit::value("1"));
        }
        assert_eq!(
            manifest
                .get_dependency_tables()
                .filter(|table| table.get("marker").is_some())
                .count(),
            4 + targets * 3
        );
        for table in manifest.get_dependency_tables_mut() {
            table.remove("marker");
        }
        group.bench_with_input(BenchmarkId::from_parameter(targets), &targets, |b, _| {
            b.iter(|| {
                for table in manifest.get_dependency_tables_mut() {
                    black_box(table);
                }
            });
        });
    }
    group.finish();
}

fn workspace_package(c: &mut Criterion) {
    let mut group = c.benchmark_group("workspace_package");
    group
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    for count in [10, 100] {
        let (_directory, mut metadata) = workspace_fixture(count, 0);
        let first = metadata.packages.first().unwrap().clone();
        let last = metadata.packages.last().unwrap().clone();
        for i in 0..count * 10 {
            let mut external = first.clone();
            external.name = cargo_metadata::PackageName::new(format!("external-{i}"));
            external.id.repr =
                format!("registry+https://github.com/rust-lang/crates.io-index#external-{i}@1.0.0");
            metadata.packages.push(external);
        }
        // A dependency with the requested name must not hide the workspace member.
        let mut external = first.clone();
        external.id.repr =
            "registry+https://github.com/rust-lang/crates.io-index#package-0@1.0.0".into();
        metadata.packages.insert(0, external);
        assert_eq!(
            cargo_utils::workspace_package(&metadata, first.name.as_str())
                .unwrap()
                .id,
            first.id
        );
        assert_eq!(
            cargo_utils::workspace_package(&metadata, last.name.as_str())
                .unwrap()
                .id,
            last.id
        );
        assert!(cargo_utils::workspace_package(&metadata, "external-0").is_err());
        for (case, name) in [
            ("first", first.name.as_str()),
            ("last", last.name.as_str()),
            ("missing", "missing"),
        ] {
            group.bench_with_input(BenchmarkId::new(case, count), &name, |b, name| {
                b.iter(|| {
                    black_box(cargo_utils::workspace_package(
                        black_box(&metadata),
                        black_box(name),
                    ))
                });
            });
        }
    }
    group.finish();
}

criterion_group!(
    benches,
    workspace_members,
    dependency_tables,
    workspace_package
);
criterion_main!(benches);
