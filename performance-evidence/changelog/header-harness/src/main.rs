#![allow(dead_code)]
use std::{hint::black_box, time::Duration};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
mod before;
mod after;

fn headers(c: &mut Criterion) {
    let mut group = c.benchmark_group("changelog_header");
    group.sample_size(50).warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(3));
    for releases in [10, 100, 1000] {
        let mut changelog = "# Changelog\n\nAll notable changes to this project.\n\n## [Unreleased]\n".to_owned();
        for index in (0..releases).rev() {
            changelog.push_str(&format!("\n## [1.{index}.0] - 2025-01-01\n\n### Fixed\n\n- Correct parsing of changelog entries (#123).\n- Preserve the original release notes.\n"));
        }
        assert_eq!(before::parse_header(&changelog), after::parse_header(&changelog));
        for (name, parse) in [("before", before::parse_header as fn(&str) -> Option<String>), ("after", after::parse_header)] {
            group.bench_with_input(BenchmarkId::new(name, releases), &changelog, |b, changelog| {
                b.iter(|| parse(black_box(changelog)));
            });
        }
    }
    let changelog = include_str!("release_plz_core_CHANGELOG.md");
    assert_eq!(before::parse_header(changelog), after::parse_header(changelog));
    for (name, parse) in [("before", before::parse_header as fn(&str) -> Option<String>), ("after", after::parse_header)] {
        group.bench_function(format!("release_plz_core/{name}"), |b| {
            b.iter(|| parse(black_box(changelog)));
        });
    }
    group.finish();
}
criterion_group!(benches, headers);
criterion_main!(benches);

#[cfg(test)]
mod parity {
    #[test]
    fn preserves_all_header_matching_cases() {
        for title in ["# Changelog", "# CHANGELOG", "# changelog", "# ChangeLog", "#    Changelog", "# Not a changelog"] {
            for newline in ["\n", "\r\n"] {
                for unreleased in ["", "## Unreleased", "## [Unreleased]", "## unreleased", "## [unreleased]", "## [UNRELEASED]", "##Unreleased", "## [Unreleased] with extra title"] {
                    for tail in ["", "\n", "\n## [1.2.3]\n\n- An entry\n", "\n## [1.2.3]\n\n## [Unreleased]\nmore text\n"] {
                        let text = format!("{title}\n\nHeader with unicode: è中文\n\n{unreleased}{tail}").replace('\n', newline);
                        assert_eq!(super::before::parse_header(&text), super::after::parse_header(&text), "{text:?}");
                    }
                }
            }
        }
    }
}
