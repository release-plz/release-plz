use std::process::Command;

use cargo_metadata::camino::Utf8Path;

const CARGO_TERM_QUIET: &str = "CARGO_TERM_QUIET";
const FALSE: &str = "false";

pub fn disable_cargo_quiet(command: &mut Command) -> &mut Command {
    command.env(CARGO_TERM_QUIET, FALSE)
}

pub fn cargo_metadata_command() -> cargo_metadata::MetadataCommand {
    let mut command = cargo_metadata::MetadataCommand::new();
    disable_cargo_metadata_quiet(&mut command);
    command
}

fn disable_cargo_metadata_quiet(
    command: &mut cargo_metadata::MetadataCommand,
) -> &mut cargo_metadata::MetadataCommand {
    command.env(CARGO_TERM_QUIET, FALSE)
}

pub fn get_manifest_metadata(
    manifest_path: &Utf8Path,
) -> Result<cargo_metadata::Metadata, cargo_metadata::Error> {
    let mut command = cargo_metadata_command();
    command.no_deps().manifest_path(manifest_path).exec()
}
