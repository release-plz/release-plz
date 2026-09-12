use cargo_metadata::{Dependency, Package};

use crate::dependency::FakeDependency;

#[derive(Clone, Debug)]
pub struct FakePackage {
    name: String,
    dependencies: Vec<FakeDependency>,
    /// The `publish` field of the manifest, as Cargo reports it.
    publish: Option<Vec<String>>,
    /// Target kinds, such as `lib`, `bin` or `example`. One target per kind.
    targets: Vec<String>,
}

impl FakePackage {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            dependencies: vec![],
            publish: None,
            targets: vec![],
        }
    }

    pub fn with_dependencies(self, dependencies: Vec<FakeDependency>) -> Self {
        Self {
            dependencies,
            ..self
        }
    }

    /// Set the `publish` field of the package.
    ///
    /// `None` is the default (publishable anywhere), `Some(vec![])` corresponds to
    /// `publish = false` in `Cargo.toml`.
    pub fn with_publish(self, publish: Option<Vec<String>>) -> Self {
        Self { publish, ..self }
    }

    /// Set the target kinds of the package, such as `lib`, `bin` or `example`.
    /// By default the package has no targets.
    pub fn with_targets(self, kinds: &[&str]) -> Self {
        Self {
            targets: kinds.iter().map(|kind| (*kind).to_string()).collect(),
            ..self
        }
    }
}

impl From<FakePackage> for Package {
    fn from(pkg: FakePackage) -> Self {
        let dependencies: Vec<Dependency> =
            pkg.dependencies.into_iter().map(Dependency::from).collect();
        let name = pkg.name;
        let targets: Vec<_> = pkg
            .targets
            .iter()
            .map(|kind| {
                serde_json::json!({
                    "name": "t", "kind": [kind], "crate_types": [kind],
                    "src_path": "/src/lib.rs", "edition": "2021", "doctest": false,
                    "test": true, "doc": true,
                })
            })
            .collect();
        serde_json::from_value(serde_json::json!({
            "name": name,
            "version": "0.1.0",
            "id": name,
            "publish": pkg.publish,
            "dependencies": dependencies,
            "features": {},
            "manifest_path": format!("{name}/Cargo.toml"),
            "targets": targets,
        }))
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_package_has_no_publish_restriction_and_no_targets() {
        let package = Package::from(FakePackage::new("pkg"));
        assert_eq!(package.publish, None);
        assert!(package.targets.is_empty());
    }

    #[test]
    fn builders_set_publish_and_targets() {
        let package = Package::from(
            FakePackage::new("pkg")
                .with_publish(Some(vec![]))
                .with_targets(&["lib", "example"]),
        );
        assert_eq!(package.publish, Some(vec![]));
        let kinds: Vec<_> = package
            .targets
            .iter()
            .map(|t| t.kind[0].to_string())
            .collect();
        assert_eq!(kinds, ["lib", "example"]);
    }
}
