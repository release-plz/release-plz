use crate::config;
use anyhow::Context;
use schemars::schema_for;
use std::path::Path;

const FOLDER: &str = ".schema";
const FILE: &str = "latest.json";

/// Generate the Schema for the configuration file, meant to be used on `SchemaStore` for IDE
/// completion
pub fn generate_schema_to_disk() -> anyhow::Result<()> {
    let file_path = Path::new(FOLDER).join(FILE);
    let json = generate_schema_json().context("can't generate schema")?;
    fs_err::create_dir_all(FOLDER)?;
    fs_err::write(file_path, json).context("can't write schema")?;
    Ok(())
}

fn generate_schema_json() -> anyhow::Result<String> {
    let schema = schema_for!(config::Config);
    let json = serde_json::to_string_pretty(&schema).context("can't convert schema to string")?;

    Ok(json)
}

#[cfg(test)]
mod tests {
    use crate::generate_schema::{FILE, FOLDER, generate_schema_json};
    use pretty_assertions::assert_eq;
    use std::env;
    use std::path::{Path, PathBuf};

    // If this test fails, run `cargo run -- generate-schema` to update the schema.
    #[test]
    fn schema_is_up_to_date() {
        let file_path = schema_path();

        // Load the two json strings
        let existing_json: String = fs_err::read_to_string(file_path).unwrap();
        let new_json = generate_schema_json().unwrap();

        // Windows-friendly comparison
        assert_eq!(
            existing_json.replace("\r\n", "\n"),
            new_json.replace("\r\n", "\n"),
            "(Hint: if change is intentional run `cargo run -- generate-schema` to update schema.)"
        );

        fn schema_path() -> PathBuf {
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

            workspace_path.join(FOLDER).join(FILE)
        }
    }

    #[test]
    fn schema_contains_id_field() {
        let json = generate_schema_json().unwrap();
        let schema: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(
            schema.get("$id"),
            Some(&serde_json::Value::String(
                "https://github.com/release-plz/release-plz/".to_string()
            )),
            "Schema should contain the $id field specified in config.rs"
        );
    }
}
