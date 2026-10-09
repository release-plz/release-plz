use std::{hash::{DefaultHasher, Hash, Hasher}, hint::black_box, time::{Duration, Instant}};
use chrono::NaiveDate;
use git_cliff_core::commit::Commit;
use changelog::ChangelogBuilder;

fn main() {
    for (name, count, prepend) in [
        ("generate_100_commits", 100, false),
        ("generate_1000_commits", 1000, false),
        ("prepend_1000_commits", 1000, true),
    ] {
        let commits = (0..count).map(|i| Commit::new(format!("{i:040x}"), format!("fix: improve release automation {i}\n\n{}", "Explain the change and its motivation. ".repeat(10)))).collect();
        let builder = ChangelogBuilder::new(commits, "1.2.3", "example")
            .with_release_date(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
        let old = format!("{}\n## [1.2.2] - 2024-01-01\n\n- Prior release\n", changelog::CHANGELOG_HEADER);
        let render = |release: changelog::Changelog<'_>| {
            if prepend { release.prepend(black_box(old.as_str())).unwrap() } else { release.generate().unwrap() }
        };
        let expected = render(builder.build());
        assert!(expected.contains("1.2.3"));
        for i in 0..count { assert!(expected.contains(&format!("improve release automation {i}\n"))); }
        if prepend { assert!(expected.ends_with("- Prior release\n")); }
        for _ in 0..5 { black_box(render(builder.build())); }
        let mut elapsed = Duration::ZERO;
        let mut iterations = 0u64;
        while elapsed < Duration::from_secs(5) {
            // The public generate/prepend APIs consume a built changelog. Keep commit
            // processing in build() outside timing to isolate that rendering boundary.
            let release = builder.build();
            let start = Instant::now();
            let output = black_box(render(black_box(release)));
            elapsed += start.elapsed();
            assert_eq!(output, expected);
            iterations += 1;
        }
        let mut hasher = DefaultHasher::new();
        expected.hash(&mut hasher);
        let output_hash = hasher.finish();
        println!("{name} output_hash={output_hash:016x} commits={count} output_bytes={} iterations={iterations} elapsed_ns={} ns_per_iter={:.3}", expected.len(), elapsed.as_nanos(), elapsed.as_nanos() as f64 / iterations as f64);
    }
}
