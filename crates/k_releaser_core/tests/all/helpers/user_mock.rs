use cargo_metadata::camino::Utf8Path;
use git_cmd::git_in_dir;

pub fn add_feature(project: &Utf8Path, message: &str) {
    fs_err::write(project.join("src").join("lib.rs"), "fn hello(){}").unwrap();
    git_in_dir(project, &["add", "."]).unwrap();
    let commit_message = format!("feat: {message}");
    git_in_dir(project, &["commit", "-m", &commit_message]).unwrap();
}
