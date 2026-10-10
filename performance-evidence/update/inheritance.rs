use std::{hint::black_box, time::Duration};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use release_plz_core::fs_utils::Utf8TempDir;

include!("inheritance_source.rs");

fn inheritance(c: &mut Criterion) {
    let mut group = c.benchmark_group("inheritance_scan");
    group.sample_size(40).warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(3));
    for (name, inherited, member_manifest) in [("independent", false, false), ("inherited", true, false), ("member_manifest", true, true)] {
        for count in [10, 100] {
            let dir = Utf8TempDir::new().unwrap();
            let manifest = dir.path().join("Cargo.toml");
            let workspace_version = if inherited { "[workspace.package]\nversion = \"1.0.0\"\n" } else { "" };
            fs_err::write(&manifest, format!("[workspace]\nmembers = [\"packages/*\"]\nresolver = \"3\"\n{workspace_version}")).unwrap();
            for index in 0..count {
                let package = dir.path().join(format!("packages/package-{index}"));
                fs_err::create_dir_all(package.join("src")).unwrap();
                let version = if inherited { "version.workspace = true" } else { "version = \"1.0.0\"" };
                fs_err::write(package.join("Cargo.toml"), format!("[package]\nname = \"package-{index}\"\n{version}\nedition = \"2024\"\n")).unwrap();
                fs_err::write(package.join("src/lib.rs"), "").unwrap();
            }
            let metadata = cargo_utils::get_manifest_metadata(&manifest).unwrap();
            let packages: Vec<_> = cargo_utils::workspace_members(&metadata).unwrap().collect();
            let selected_manifest = if member_manifest { dir.path().join("packages/package-0/Cargo.toml") } else { manifest.clone() };
            let (version, packages_inheriting) = inheritance_scan(&packages, &selected_manifest).unwrap();
            assert_eq!(version.is_some(), inherited && !member_manifest);
            assert_eq!(packages_inheriting.len(), if inherited { count } else { 0 });
            group.bench_with_input(BenchmarkId::new(name, count), &count, |b, _| {
                b.iter(|| inheritance_scan(black_box(&packages), black_box(&selected_manifest)).unwrap());
            });
        }
    }
    group.finish();
}
criterion_group!(benches, inheritance);
criterion_main!(benches);
