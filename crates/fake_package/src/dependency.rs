use cargo_metadata::{Dependency, DependencyKind};

#[derive(Clone, Debug)]
#[must_use]
pub struct FakeDependency {
    name: String,
    kind: DependencyKind,
}

impl FakeDependency {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: DependencyKind::Normal,
        }
    }

    pub fn dev(self) -> Self {
        Self {
            kind: DependencyKind::Development,
            ..self
        }
    }
}

impl From<FakeDependency> for Dependency {
    fn from(dep: FakeDependency) -> Self {
        serde_json::from_value(serde_json::json!({
            "name": dep.name,
            "req": "0.1.0",
            "kind": dep.kind,
            "optional": false,
            "uses_default_features": true,
            "features": [],
        }))
        .unwrap()
    }
}
