#![allow(dead_code)]
use std::{hint::black_box, time::Instant};

use git_cliff_core::{commit::Signature, contributor::RemoteContributor};

// This enum is needed only by the unrelated Diff type in the included source.
// Commit and both measured methods come unchanged from the actual source files.
mod semver_check {
    #[derive(Debug, Clone)]
    pub enum SemverCheck {
        Skipped,
    }
}

#[path = "../baseline/diff.rs"]
mod before;
#[path = "../candidate/diff.rs"]
mod after;

fn measure(revision: &str, name: &str, expected: bool, check: impl Fn() -> bool) {
    assert_eq!(check(), expected);
    for _ in 0..10_000 {
        black_box(check());
    }
    // Bound calls explicitly: git-cliff's original conversion leaks a message
    // per call, so an adaptive benchmark could otherwise consume unlimited RAM.
    let iterations = 300_000;
    let started = Instant::now();
    for _ in 0..iterations {
        black_box(check());
    }
    let elapsed = started.elapsed();
    println!("{{\"revision\":\"{revision}\",\"case\":\"{name}\",\"iterations\":{iterations},\"elapsed_ns\":{},\"ns_per_call\":{}}}", elapsed.as_nanos(), elapsed.as_nanos() as f64 / iterations as f64);
}

fn main() {
    let revision = std::env::args().nth(1).expect("before or after");
    assert!(matches!(revision.as_str(), "before" | "after"));
    let body = format!("feat: add behavior\n\n{}\n\nBREAKING CHANGE: update API", "Implementation details. ".repeat(40));
    for (name, message, expected) in [
        ("conventional", "feat: add behavior", true),
        ("body_1k", body.as_str(), true),
        ("nonconventional", "Update the documentation for the new API", false),
    ] {
        let author = Signature {
            name: Some("Example Contributor".to_string()),
            email: Some("contributor@example.test".to_string()),
            timestamp: 1_700_000_000,
        };
        let remote = RemoteContributor {
            username: Some("contributor".to_string()),
            pr_number: Some(123),
            ..Default::default()
        };
        if revision == "before" {
            let mut commit = before::Commit::new("0123456789012345678901234567890123456789".to_string(), message.to_string());
            commit.author = author.clone();
            commit.committer = author;
            commit.remote = remote;
            measure(&revision, name, expected, || black_box(&commit).is_conventional());
        } else {
            let mut commit = after::Commit::new("0123456789012345678901234567890123456789".to_string(), message.to_string());
            commit.author = author.clone();
            commit.committer = author;
            commit.remote = remote;
            measure(&revision, name, expected, || black_box(&commit).is_conventional());
        }
    }
}
