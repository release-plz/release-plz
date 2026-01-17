use std::collections::HashMap;

use cargo_metadata::camino::Utf8PathBuf;

pub struct OldChangelogs {
    old_changelogs: HashMap<Utf8PathBuf, String>,
}

impl OldChangelogs {
    pub fn new() -> Self {
        Self {
            old_changelogs: HashMap::new(),
        }
    }

    pub fn get_or_read(&self, changelog_path: &Utf8PathBuf) -> Option<String> {
        self.old_changelogs
            .get(changelog_path)
            .cloned()
            .or(fs_err::read_to_string(changelog_path).ok())
    }

    pub fn insert(&mut self, changelog_path: Utf8PathBuf, changelog: String) {
        self.old_changelogs.insert(changelog_path, changelog);
    }
}
