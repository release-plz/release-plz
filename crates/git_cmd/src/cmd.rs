use anyhow::Context;

pub(crate) fn string_from_bytes(bytes: Vec<u8>) -> anyhow::Result<String> {
    let stdout = String::from_utf8(bytes).context("cannot extract stderr")?;
    let stdout = stdout.trim();
    Ok(stdout.to_string())
}
