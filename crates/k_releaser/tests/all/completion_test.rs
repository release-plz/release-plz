use clap::ValueEnum;
use clap_complete::Shell;

use crate::helpers;

#[test]
fn test_generate_completions() {
    for &shell in Shell::value_variants() {
        helpers::cmd::release_plz_cmd()
            .arg("generate-completions")
            .arg(shell.to_string())
            .assert()
            .success();
    }
}
