use cargo_metadata::camino::Utf8Path;
use serde::Deserialize;
use tracing::warn;

#[derive(Deserialize)]
struct CargoVcsInfo {
    git: Git,
}

#[derive(Deserialize)]
struct Git {
    sha1: String,
}

pub fn read_sha1_from_cargo_vcs_info(cargo_vcs_info_path: &Utf8Path) -> Option<String> {
    if let Ok(cargo_vcs_info) = fs_err::read_to_string(cargo_vcs_info_path) {
        match serde_json::from_str::<CargoVcsInfo>(&cargo_vcs_info) {
            Ok(info) => Some(info.git.sha1),
            Err(e) => {
                warn!("failed to parse .cargo_vcs_info.json: {}", e);
                None
            }
        }
    } else {
        None
    }
}
