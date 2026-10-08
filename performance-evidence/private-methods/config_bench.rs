
#[cfg(test)]
mod performance_experiment {
    use super::*;
    use std::{hint::black_box, time::Instant};

    #[test]
    #[ignore = "temporary benchmark; excluded from the performance PR"]
    fn bench_config_lookup() {
        let default = UpdateConfig {
            changelog_path: Some("CHANGELOG.md".into()),
            tag_name_template: Some("{{ package }}-v{{ version }}".into()),
            custom_minor_increment_regex: Some("feat|enhancement".into()),
            custom_major_increment_regex: Some("breaking|major".into()),
            ..UpdateConfig::default()
        };
        let mut configs = PackagesConfig::default();
        configs.set_default(default.clone());
        let mut overridden: PackageUpdateConfig = default.clone().into();
        overridden.generic.changelog_update = false;
        configs.set("overridden-package".into(), overridden.clone());
        assert_eq!(configs.get("overridden-package"), overridden);
        assert_eq!(configs.get("default-package"), default.into());
        for name in ["overridden-package", "default-package"] {
            for _ in 0..10_000 {
                black_box(configs.get(black_box(name)));
            }
            let iterations = 8_000_000;
            let start = Instant::now();
            for _ in 0..iterations {
                black_box(black_box(&configs).get(black_box(name)));
            }
            println!("config_lookup/{name}: {:.3} ns/iteration ({iterations} iterations)", start.elapsed().as_nanos() as f64 / iterations as f64);
        }
    }
}
