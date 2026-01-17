mod helpers;
mod release_pr;
mod update;

use std::process::Command;

use cargo_metadata::camino::Utf8Path;
use git_cmd::Repo;

fn init_project(project: &Utf8Path) {
    Command::new("cargo")
        .arg("new")
        .arg(project)
        .output()
        .unwrap();

    Repo::init(project);
}
