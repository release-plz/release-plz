use std::collections::{HashMap, HashSet};

use cargo_metadata::camino::Utf8PathBuf;

pub struct OldChangelogs {
    old_changelogs: HashMap<Utf8PathBuf, OldChangelog>,
    /// Changelogs that more than one workspace package writes to.
    shared_paths: HashSet<Utf8PathBuf>,
}

impl OldChangelogs {
    pub fn new(shared_paths: HashSet<Utf8PathBuf>) -> Self {
        Self {
            old_changelogs: HashMap::new(),
            shared_paths,
        }
    }

    pub fn get_or_read(&mut self, changelog_path: &Utf8PathBuf) -> &mut OldChangelog {
        self.old_changelogs
            .entry(changelog_path.clone())
            .or_insert_with(|| {
                OldChangelog::new(
                    fs_err::read_to_string(changelog_path).ok(),
                    self.shared_paths.contains(changelog_path),
                )
            })
    }
}

/// Keep release history separate from entries generated for other packages in this run.
pub struct OldChangelog {
    original: Option<String>,
    updated: Option<String>,
    /// Whether the changelog contains releases of other packages too.
    is_shared: bool,
}

impl OldChangelog {
    pub fn new(original: Option<String>, is_shared: bool) -> Self {
        Self {
            original,
            updated: None,
            is_shared,
        }
    }

    pub fn original(&self) -> Option<&str> {
        self.original.as_deref()
    }

    pub fn current(&self) -> Option<&str> {
        self.updated.as_deref().or(self.original())
    }

    pub fn is_shared(&self) -> bool {
        self.is_shared
    }

    pub fn update(&mut self, changelog: String) {
        self.updated = Some(changelog);
    }
}
