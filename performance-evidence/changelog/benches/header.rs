use std::{hint::black_box, time::Duration};
use changelog_header_evidence::parse_header;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

fn headers(c: &mut Criterion) {
    let mut group = c.benchmark_group("changelog_header");
    group.sample_size(100).warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(3));
    for (name, introduction, releases) in [
        ("short", "All notable changes.\n".to_owned(), 1),
        ("long_header", "All notable changes to this project are documented here.\n".repeat(80), 1),
        ("history_100", "All notable changes.\n".to_owned(), 100),
    ] {
        let expected = format!("# Changelog\n\n{introduction}");
        let mut input = expected.clone();
        for release in (0..releases).rev() {
            input.push_str(&format!("\n## [1.0.{release}] - 2025-01-01\n\n### Fixed\n\n- Fixed a bug.\n"));
        }
        assert_eq!(parse_header(&input).as_deref(), Some(expected.as_str()));
        group.bench_with_input(BenchmarkId::new("without_unreleased", name), &input, |b, input| {
            b.iter(|| black_box(parse_header(black_box(input))));
        });
    }
    let input = "# Changelog\n\n## [Unreleased]\n\n## [1.0.0]\n\n- Fixed a bug.\n";
    assert_eq!(parse_header(input).as_deref(), Some("# Changelog\n\n## [Unreleased]\n"));
    group.bench_function("unreleased_control", |b| b.iter(|| black_box(parse_header(black_box(input)))));
    group.finish();
}
criterion_group!(benches, headers);
criterion_main!(benches);
