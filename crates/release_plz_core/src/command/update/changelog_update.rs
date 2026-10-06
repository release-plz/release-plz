use std::collections::HashMap;

use cargo_metadata::camino::Utf8PathBuf;

pub struct OldChangelogs {
    old_changelogs: HashMap<Utf8PathBuf, OldChangelog>,
}

impl OldChangelogs {
    pub fn new() -> Self {
        Self {
            old_changelogs: HashMap::new(),
        }
    }

    pub fn get_or_read(&mut self, changelog_path: &Utf8PathBuf) -> &mut OldChangelog {
        self.old_changelogs
            .entry(changelog_path.clone())
            .or_insert_with(|| OldChangelog::new(fs_err::read_to_string(changelog_path).ok()))
    }
}

/// Keep release history separate from entries generated for other packages in this run.
pub struct OldChangelog {
    original: Option<String>,
    updated: Option<String>,
}

impl OldChangelog {
    pub fn new(original: Option<String>) -> Self {
        Self {
            original,
            updated: None,
        }
    }

    pub fn original(&self) -> Option<&str> {
        self.original.as_deref()
    }

    pub fn current(&self) -> Option<&str> {
        self.updated.as_deref().or(self.original())
    }

    pub fn update(&mut self, changelog: String) {
        self.updated = Some(changelog);
    }
}
