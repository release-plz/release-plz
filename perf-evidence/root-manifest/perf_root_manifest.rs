use std::{hint::black_box, time::Instant};

use cargo_metadata::{Package, semver::Version};
use release_plz_core::fs_utils::Utf8TempDir;

// Adapted from PR #3146's manifest_updates fixture: the workspace root is a
// package that depends on every member, including the one being updated.
struct Fixture {
    directory: Utf8TempDir,
    packages: Vec<Package>,
}

impl Fixture {
    fn new(count: usize) -> Self {
        let directory = Utf8TempDir::new().unwrap();
        let manifest = directory.path().join("Cargo.toml");
        let mut root = String::from(
            "[workspace]\nmembers = [\"packages/*\"]\nresolver = \"3\"\n\
             [package]\nname = \"root-app\"\nversion = \"1.0.0\"\nedition = \"2024\"\n\
             [dependencies]\n",
        );
        fs_err::create_dir_all(directory.path().join("src")).unwrap();
        fs_err::write(directory.path().join("src/lib.rs"), "").unwrap();
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
            root.push_str(&format!(
                "package-{index} = {{ path = \"packages/package-{index}\", version = \"1.0.0\" }}\n"
            ));
        }
        fs_err::write(&manifest, root).unwrap();
        let metadata = cargo_utils::get_manifest_metadata(&manifest).unwrap();
        let packages = cargo_utils::workspace_members(&metadata).unwrap().collect();
        Self {
            directory,
            packages,
        }
    }

    fn verify(&self, version: &Version, count: usize) {
        let root =
            cargo_utils::LocalManifest::try_new(&self.directory.path().join("Cargo.toml")).unwrap();
        let leaf = cargo_utils::LocalManifest::try_new(
            &self.directory.path().join("packages/package-0/Cargo.toml"),
        )
        .unwrap();
        assert_eq!(
            leaf.data["package"]["version"].as_str(),
            Some(version.to_string().as_str())
        );
        assert_eq!(root.data["package"]["version"].as_str(), Some("1.0.0"));
        assert_eq!(
            root.data["dependencies"]["package-0"]["version"].as_str(),
            Some(version.to_string().as_str())
        );
        for index in 1..count {
            assert_eq!(
                root.data["dependencies"][&format!("package-{index}")]["version"].as_str(),
                Some("1.0.0")
            );
        }
    }
}

fn main() {
    for (count, iterations) in [(10, 10_000), (100, 3_000)] {
        let fixture = Fixture::new(count);
        let packages: Vec<_> = fixture.packages.iter().collect();
        let manifest = fixture.directory.path().join("Cargo.toml");
        let package = fixture.directory.path().join("packages/package-0");
        let versions = [Version::new(1, 0, 1), Version::new(1, 0, 2)];
        for iteration in 0..100 {
            release_plz_core::set_version(&packages, &package, &versions[iteration % 2], &manifest)
                .unwrap();
        }
        fixture.verify(&versions[1], count);
        let start = Instant::now();
        for iteration in 0..iterations {
            release_plz_core::set_version(
                black_box(&packages),
                &package,
                &versions[iteration % 2],
                &manifest,
            )
            .unwrap();
        }
        let elapsed = start.elapsed();
        fixture.verify(&versions[(iterations - 1) % 2], count);
        println!(
            "root_package/{count}: {:.3} us/iteration; iterations={iterations}; elapsed={:.6}s",
            elapsed.as_secs_f64() * 1e6 / iterations as f64,
            elapsed.as_secs_f64(),
        );
    }
}
