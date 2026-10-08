#[cfg(test)]
mod template_performance {
    use super::*;
    use std::{hint::black_box, time::{Duration, Instant}};

    #[test]
    #[ignore]
    fn benchmark_release_templates() {
        for (name, multiple, custom) in [("single_default", false, false), ("workspace_default", true, false), ("custom_control", true, true)] {
            let mut project = Project {
                packages: vec![], release_metadata: HashMap::new(), root: Utf8PathBuf::new(), manifest_dir: Utf8PathBuf::new(), contains_multiple_releasable_packages: multiple,
            };
            // Project::new keeps metadata for each releasable package, even
            // when both template overrides are absent.
            project.release_metadata.insert("example".to_owned(), ReleaseMetadata {
                tag_name_template: custom.then(|| "{{ package }}-v{{ version }}".to_owned()),
                release_name_template: custom.then(|| "{{ package }}-v{{ version }}".to_owned()),
            });
            let expected = if multiple { "example-v1.2.3" } else { "v1.2.3" };
            assert_eq!(project.git_tag("example", "1.2.3").unwrap(), expected);
            assert_eq!(project.release_name("example", "1.2.3").unwrap(), expected);
            let operation = || {
                black_box(black_box(&project).git_tag(black_box("example"), black_box("1.2.3")).unwrap());
                black_box(black_box(&project).release_name(black_box("example"), black_box("1.2.3")).unwrap());
            };
            for _ in 0..100 { operation(); }
            let start = Instant::now();
            let mut iterations: u64 = 0;
            while start.elapsed() < Duration::from_secs(2) {
                for _ in 0..100 { operation(); }
                iterations += 100;
            }
            println!("{name}: iterations={iterations}, ns_per_pair={:.3}", start.elapsed().as_nanos() as f64 / iterations as f64);
        }
    }
}
