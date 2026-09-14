use std::process::Command;

use anyhow::Context as _;
use cargo_metadata::{Package, camino::Utf8Path};

const CARGO_TERM_QUIET: &str = "CARGO_TERM_QUIET";
const FALSE: &str = "false";

/// Disable cargo's quiet mode, to improve the debugging experience when cargo fails.
/// Plus, release-plz parses cargo's output to determine what happened.
pub fn disable_cargo_quiet(command: &mut Command) -> &mut Command {
    command.env(CARGO_TERM_QUIET, FALSE)
}

pub fn cargo_metadata_command() -> cargo_metadata::MetadataCommand {
    let mut command = cargo_metadata::MetadataCommand::new();
    disable_cargo_metadata_quiet(&mut command);
    command
}

/// Disable cargo's quiet mode, to improve the debugging experience when `cargo_metadata` fails.
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

/// Find a workspace member by name.
pub fn workspace_package<'a>(
    metadata: &'a cargo_metadata::Metadata,
    package_name: &str,
) -> anyhow::Result<&'a Package> {
    metadata
        .workspace_packages()
        .into_iter()
        .find(|package| package.name == package_name)
        .with_context(|| {
            format!(
                "cannot find package {package_name:?} in workspace {:?}",
                metadata.workspace_root
            )
        })
}
