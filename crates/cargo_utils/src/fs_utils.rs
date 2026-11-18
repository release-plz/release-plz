use std::path::PathBuf;

use cargo_metadata::camino::Utf8PathBuf;

pub fn to_utf8_pathbuf(path: PathBuf) -> anyhow::Result<Utf8PathBuf> {
    match Utf8PathBuf::from_path_buf(path) {
        Ok(p) => Ok(p),
        Err(path) => Err(anyhow::anyhow!(
            "cannot convert {} to Utf8Path",
            path.as_os_str().as_encoded_bytes().escape_ascii()
        )),
    }
}
