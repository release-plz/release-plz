
#[cfg(test)]
mod performance_experiment {
    use super::*;
    use std::{hint::black_box, time::Instant};

    #[test]
    #[ignore = "temporary benchmark; excluded from the performance PR"]
    fn bench_workspace_version() {
        let manifest = Utf8Path::new("../../tests/fixtures/typo-in-overrides/Cargo.toml");
        let metadata = cargo_utils::get_manifest_metadata(manifest).unwrap();
        let req = UpdateRequest::new(metadata).unwrap();
        let project = Project::new(req.local_manifest(), None, &HashSet::new(), req.cargo_metadata(), &req).unwrap();
        let updater = Updater { project: &project, req: &req };
        let temp = crate::fs_utils::Utf8TempDir::new().unwrap();
        let shared_manifest = temp.path().join("Cargo.toml");
        fs_err::write(&shared_manifest, "[workspace]\n[workspace.package]\nversion = \"1.0.0\"\n").unwrap();
        for count in [100, 1000] {
            let packages: Vec<Package> = (0..count).map(|index| {
                let mut p: Package = fake_package::FakePackage::new(format!("package-{index}")).into();
                p.version = Version::new(1, 0, 0);
                p
            }).collect();
            let diffs: Vec<_> = packages.iter().map(|p| (p, Diff::new(true))).collect();
            for step in [1, 4, 0] {
                let inherited: HashSet<String> = if step == 0 { HashSet::new() } else { packages.iter().step_by(step).map(|p| p.name.to_string()).collect() };
                let actual = updater.new_workspace_version(&shared_manifest, &diffs, &inherited).unwrap();
                assert_eq!(actual, (step != 0).then(|| Version::new(1, 0, 0)));
                for _ in 0..10 {
                    black_box(updater.new_workspace_version(&shared_manifest, &diffs, &inherited).unwrap());
                }
                let iterations = if step == 0 { 100_000 } else if count == 100 { 10_000 } else { 1_000 };
                let start = Instant::now();
                for _ in 0..iterations {
                    black_box(black_box(&updater).new_workspace_version(black_box(&shared_manifest), black_box(&diffs), black_box(&inherited)).unwrap());
                }
                println!("workspace_version/packages_{count}_inherited_{}: {:.3} us/iteration ({iterations} iterations)", inherited.len(), start.elapsed().as_nanos() as f64 / iterations as f64 / 1000.0);
            }
        }
    }
}
