use cargo_metadata::camino::Utf8Path;
use cargo_utils::LocalManifest;

use super::TEST_REGISTRY;

pub struct TestPackage {
    pub name: String,
    type_: PackageType,
    path_dependencies: Vec<String>,
    is_workspace_member: bool,
}

pub enum PackageType {
    Bin,
    Lib,
}

impl TestPackage {
    pub fn new(name: impl AsRef<str>) -> Self {
        Self {
            name: name.as_ref().to_string(),
            type_: PackageType::Bin,
            path_dependencies: vec![],
            is_workspace_member: false,
        }
    }

    pub fn as_workspace_member(self) -> Self {
        Self {
            is_workspace_member: true,
            ..self
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
        edit_cargo_toml(crate_dir, self.is_workspace_member);
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

fn edit_cargo_toml(repo_dir: &Utf8Path, is_workspace_member: bool) {
    let cargo_toml_path = repo_dir.join("Cargo.toml");
    let mut cargo_toml = LocalManifest::try_new(&cargo_toml_path).unwrap();

    // Set publish registry
    let mut registry_array = toml_edit::Array::new();
    registry_array.push(TEST_REGISTRY);
    cargo_toml.data["package"]["publish"] =
        toml_edit::Item::Value(toml_edit::Value::Array(registry_array));

    // If this is a workspace member, make it inherit the workspace version
    if is_workspace_member {
        cargo_toml.data["package"]["version"] =
            toml_edit::Item::Value(toml_edit::Value::Boolean(toml_edit::Formatted::new(true)));
        cargo_toml.data["package"]["version"]
            .as_value_mut()
            .unwrap()
            .decor_mut()
            .set_suffix("");
        // Use inline table syntax for workspace inheritance
        let mut version_table = toml_edit::InlineTable::new();
        version_table.insert("workspace", true.into());
        cargo_toml.data["package"]["version"] = toml_edit::value(version_table);
    }

    cargo_toml.write().unwrap();
}
