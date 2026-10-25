#[derive(PartialEq, Eq, Hash, Ord, PartialOrd, Clone, Debug, Copy)]
pub enum DepKind {
    Normal,
    Development,
    Build,
}

impl DepKind {
    const ALL: [Self; 3] = [Self::Normal, Self::Development, Self::Build];

    /// Name of the manifest table that declares dependencies of this kind.
    fn table_name(self) -> &'static str {
        match self {
            Self::Normal => "dependencies",
            Self::Development => "dev-dependencies",
            Self::Build => "build-dependencies",
        }
    }

    /// The kind of dependencies declared by a table named `key`, if it is a dependency table.
    pub(crate) fn from_table_name(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.table_name() == key)
    }
}
