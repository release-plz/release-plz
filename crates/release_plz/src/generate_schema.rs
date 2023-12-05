use crate::config;
use schemars::schema_for;
use std::fs;
use std::path::Path;

const FOLDER: &str = ".schema";
const FILE: &str = "latest.json";

/// Generate the Schema for the configuration file, meant to be used on `SchemaStore` for IDE
/// completion
pub fn generate_schema_to_disk() -> anyhow::Result<()> {
    let file_path = Path::new(FOLDER).join(FILE);
    let json = generate_schema_json()?;
    fs::create_dir_all(FOLDER)?;
    fs::write(file_path, json)?;
    Ok(())
}

fn generate_schema_json() -> anyhow::Result<String> {
    const SCHEMA_TOKEN: &str = r##"schema#","##;
    const ID: &str = r##""$id": "https://github.com/MarcoIeni/release-plz/"##;

    let schema = schema_for!(config::Config);
    let mut json = serde_json::to_string_pretty(&schema)?;
    // As of now, Schemars does not support the $id field, so we insert it manually.
    // See here for update on resolution: https://github.com/GREsau/schemars/issues/229
    json = json.replace(
        SCHEMA_TOKEN,
        &format!("{}\n  {}{}/{}\",", SCHEMA_TOKEN, ID, FOLDER, FILE),
    );

    Ok(json)
}

#[cfg(test)]
mod tests {
    use crate::generate_schema::{generate_schema_json, FILE, FOLDER};
    use std::path::Path;
    use std::{env, fs};

    #[test]
    fn schema_is_up_to_date() {
        // Let's get the root workspace folder
        let output = std::process::Command::new(env!("CARGO"))
            .arg("locate-project")
            .arg("--workspace")
            .arg("--message-format=plain")
            .output()
            .unwrap()
            .stdout;

        let workspace_path = Path::new(std::str::from_utf8(&output).unwrap().trim())
            .parent()
            .unwrap();

        let file_path = workspace_path.join(FOLDER).join(FILE);

        // Load the two json strings
        let existing_json: String = fs::read_to_string(file_path).unwrap();
        let new_json = generate_schema_json().unwrap();

        // Windows-friendly comparison
        assert_eq!(
            existing_json.replace("\r\n", "\n"),
            new_json.replace("\r\n", "\n")
        );
    }
}