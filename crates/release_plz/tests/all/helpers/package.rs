use cargo_metadata::camino::Utf8Path;
use cargo_utils::LocalManifest;

use super::TEST_REGISTRY;

pub struct TestPackage {
    pub name: String,
    type_: PackageType,
    path_dependencies: Vec<String>,
}

pub enum PackageType {
    Bin,
    Lib,
}

impl TestPackage {
    pub fn new(name: impl AsRef<str>) -> TestPackage {
        Self {
            name: name.as_ref().to_string(),
            type_: PackageType::Bin,
            path_dependencies: vec![],
        }
    }

    pub fn with_type(self, type_: PackageType) -> Self {
        Self { type_, ..self }
    }

    pub fn with_path_dependencies<I: Into<String>>(self, path_dependencies: Vec<I>) -> Self {
        let path_dependencies: Vec<String> =
            path_dependencies.into_iter().map(|d| d.into()).collect();
        Self {
            path_dependencies,
            ..self
        }
    }

    pub fn cargo_init(&self, crate_dir: &Utf8Path) {
        let args = match self.type_ {
            PackageType::Bin => vec!["init"],
            PackageType::Lib => vec!["init", "--lib"],
        };
        assert_cmd::Command::new("cargo")
            .current_dir(crate_dir)
            .args(&args)
            .assert()
            .success();
        edit_cargo_toml(crate_dir);
    }

    pub fn write_dependencies(&self, crate_dir: &Utf8Path) {
        for dep in &self.path_dependencies {
            assert_cmd::Command::new("cargo")
                .current_dir(crate_dir)
                .args(["add", "--path", dep])
                .assert()
                .success();
        }
    }
}

fn edit_cargo_toml(repo_dir: &Utf8Path) {
    let cargo_toml_path = repo_dir.join("Cargo.toml");
    let mut cargo_toml = LocalManifest::try_new(&cargo_toml_path).unwrap();
    let mut registry_array = toml_edit::Array::new();
    registry_array.push(TEST_REGISTRY);
    cargo_toml.data["package"]["publish"] =
        toml_edit::Item::Value(toml_edit::Value::Array(registry_array));
    cargo_toml.write().unwrap();
}
