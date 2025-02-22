use std::io::stdout;

use clap::CommandFactory;
use clap_complete::Shell;

use crate::CliArgs;

/// Generate command autocompletions for various shells.
#[derive(clap::Parser, Debug)]
pub struct Completions {
    /// Shell option
    #[arg(default_value = "bash")]
    shell: Shell,
}

impl Completions {
    pub fn run(&self) {
        clap_complete::generate(
            self.shell,
            &mut CliArgs::command(),
            "release-plz",
            &mut stdout(),
        );
    }
}
