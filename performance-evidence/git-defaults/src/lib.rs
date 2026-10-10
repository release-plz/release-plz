#![allow(dead_code)]
pub const NO_COMMIT_ID: &str = "0000000";
mod changelog_parser;
pub mod implementation {
    include!("changelog.rs");

    // Benchmark the production helper without changing its visibility in the PR.
    pub fn benchmark_apply_defaults(config: GitConfig, pr_link: Option<&str>) -> GitConfig {
        apply_defaults_to_git_config(config, pr_link)
    }
}
