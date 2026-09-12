#[derive(PartialEq, Eq, Hash, Ord, PartialOrd, Clone, Debug, Copy)]
pub enum DepKind {
    Normal,
    Development,
    Build,
}

/// Dependency table to add dep to
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepTable {
    kind: DepKind,
    target: Option<String>,
}

impl DepTable {
    pub const KINDS: &'static [Self] = &[
        Self::new().set_kind(DepKind::Normal),
        Self::new().set_kind(DepKind::Development),
        Self::new().set_kind(DepKind::Build),
    ];

    /// Reference to a Dependency Table
    pub(crate) const fn new() -> Self {
        Self {
            kind: DepKind::Normal,
            target: None,
        }
    }

    /// Choose the type of dependency
    pub(crate) const fn set_kind(mut self, kind: DepKind) -> Self {
        self.kind = kind;
        self
    }

    /// Choose the platform for the dependency
    pub(crate) fn set_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// The type of dependencies stored in this table.
    pub fn kind(&self) -> DepKind {
        self.kind
    }

    pub(crate) fn kind_table(&self) -> &str {
        match self.kind {
            DepKind::Normal => "dependencies",
            DepKind::Development => "dev-dependencies",
            DepKind::Build => "build-dependencies",
        }
    }
}

impl Default for DepTable {
    fn default() -> Self {
        Self::new()
    }
}

impl From<DepKind> for DepTable {
    fn from(other: DepKind) -> Self {
        Self::new().set_kind(other)
    }
}
