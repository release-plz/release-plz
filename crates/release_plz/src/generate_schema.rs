use crate::config;
use schemars::schema_for;
use std::fs;

/// Generate the Schema for the configuration file, meant to be used on SchemaStore for IDE
/// completion
pub fn generate_schema() {
    const SCHEMA_TOKEN: &str = r##"schema#","##;
    const ID: &str = r##""$id": "https://github.com/MarcoIeni/release-plz/"##;
    const FOLDER: &str = ".schema/";
    const FILE: &str = "latest.json";

    let schema = schema_for!(config::Config);
    let mut json = serde_json::to_string_pretty(&schema).unwrap();
    // As of now, Schemars does not support the $id field, so we insert it manually.
    // See here for update on resolution: https://github.com/GREsau/schemars/issues/229
    json = json.replace(
        SCHEMA_TOKEN,
        &format!("{}\n  {}{}{}\",", SCHEMA_TOKEN, ID, FOLDER, FILE),
    );
    fs::create_dir_all(FOLDER).unwrap();
    fs::write(&format!("{}{}", FOLDER, FILE), json).unwrap();
}
