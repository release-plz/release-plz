use cargo_metadata::{Dependency, DependencyKind};

#[derive(Clone, Debug)]
pub struct FakeDependency {
    name: String,
    kind: DependencyKind,
    req: String,
}

impl FakeDependency {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: DependencyKind::Normal,
            req: "0.1.0".to_string(),
        }
    }

    pub fn dev(self) -> Self {
        Self {
            kind: DependencyKind::Development,
            ..self
        }
    }

    /// Create a dev-dependency without a version requirement (path-only).
    /// These are stripped from the published manifest by cargo.
    pub fn unversioned_dev(self) -> Self {
        Self {
            kind: DependencyKind::Development,
            req: "*".to_string(),
            ..self
        }
    }
}

impl From<FakeDependency> for Dependency {
    fn from(dep: FakeDependency) -> Self {
        serde_json::from_value(serde_json::json!({
            "name": dep.name,
            "req": dep.req,
            "kind": dep.kind,
            "optional": false,
            "uses_default_features": true,
            "features": [],
        }))
        .unwrap()
    }
}
