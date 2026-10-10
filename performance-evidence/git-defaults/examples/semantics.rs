use changelog_git_defaults_evidence::implementation;
use git_cliff_core::config::{LinkParser, TextProcessor};
use regex::Regex;

fn main() {
    for link in [None, Some("https://github.com/example/repository/pull")] {
        for fields in 0..16 {
            let mut config = implementation::default_git_config(link);
            if fields & 1 == 0 {
                config.commit_parsers.clear();
            } else {
                config.commit_parsers.truncate(1);
                config.commit_parsers[0].group = Some("Custom features".into());
            }
            config.commit_preprocessors = if fields & 2 == 0 {
                vec![]
            } else {
                vec![TextProcessor { pattern: Regex::new("custom").unwrap(), replace: Some("message".into()), replace_command: None }]
            };
            config.sort_commits = if fields & 4 == 0 { "" } else { "oldest" }.into();
            config.link_parsers = if fields & 8 == 0 {
                vec![]
            } else {
                vec![LinkParser { pattern: Regex::new(r"#(\d+)").unwrap(), href: "https://example.com/issue/$1".into(), text: Some("#$1".into()) }]
            };
            println!("{link:?}/{fields}: {:?}", implementation::benchmark_apply_defaults(config, link));
        }
    }
}
