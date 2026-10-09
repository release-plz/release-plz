#!/usr/bin/env python3
"""Inject temporary in-module benchmarks; never include this patch in a PR.

Snapshots are factored verbatim out of get_packages_diffs solely for measurement.
Group measurements call the actual get_version_groups method.
"""
from pathlib import Path
import sys

root = Path(sys.argv[1])
p = root / 'crates/release_plz_core/src/command/update/updater.rs'
s = p.read_text()
start = s.index('        let packages_commits: HashMap<String, Vec<Commit>>')
comment = '        // Snapshot only histories another package includes before adding commits to diffs.'
if comment in s:
    start = s.index(comment)
end = s.index('\n\n        let check_semver', start)
body = s[start:end]
s = s[:start] + '        let packages_commits = self.perf_snapshot_commits(&packages_diffs);' + s[end:]
method = '''    fn perf_snapshot_commits(&self, packages_diffs: &[(&Package, Diff)]) -> HashMap<String, Vec<Commit>> {
''' + body + '''
        packages_commits
    }

'''
pos = s.index('    /// Get the highest next version')
s = s[:pos] + method + s[pos:]
s += '''
#[cfg(test)]
mod perf_update_bench {
    use super::*;
    use std::{hint::black_box, time::{Duration, Instant}};

    fn measure(name: &str, mut f: impl FnMut()) {
        let warmup = Instant::now();
        while warmup.elapsed() < Duration::from_millis(100) {
            f();
        }
        let started = Instant::now();
        let mut iterations: u64 = 0;
        loop {
            for _ in 0..32 {
                f();
                iterations += 1;
            }
            if started.elapsed() >= Duration::from_millis(500) {
                break;
            }
        }
        let elapsed = started.elapsed();
        println!("BENCH {name} {} {iterations}", elapsed.as_nanos() as f64 / iterations as f64);
    }

    fn request() -> UpdateRequest {
        UpdateRequest::new(fake_package::metadata::fake_metadata()).unwrap()
    }

    fn project(req: &UpdateRequest) -> Project {
        Project::new(req.local_manifest(), None, &HashSet::new(), req.cargo_metadata(), req).unwrap()
    }

    #[test]
    #[ignore]
    fn version_groups() {
        for count in [10, 100] {
            for custom in [false, true] {
                let mut config = crate::UpdateConfig::default();
                if custom {
                    config.custom_minor_increment_regex = Some("^(feat|feature|enhancement)$".to_owned());
                    config.custom_major_increment_regex = Some("^(breaking|major)$".to_owned());
                }
                let req = request().with_default_package_config(config);
                let project = project(&req);
                let updater = Updater { project: &project, req: &req };
                let packages: Vec<Package> = (0..count)
                    .map(|i| fake_package::FakePackage::new(format!("package-{i}")).into())
                    .collect();
                let diffs: Vec<_> = packages.iter().map(|p| (p, Diff::new(true))).collect();
                assert!(updater.get_version_groups(&diffs).unwrap().is_empty());
                measure(&format!("groups_{count}_custom_{custom}"), || {
                    black_box(updater.get_version_groups(black_box(&diffs)).unwrap());
                });
            }
        }
    }

    #[test]
    #[ignore]
    fn include_snapshots() {
        for (count, commits) in [(10, 100), (100, 100), (100, 0)] {
            for include in ["none", "one", "all"] {
                let mut req = request();
                if include != "none" {
                    req = req.with_package_config("package-0", crate::PackageUpdateConfig {
                        changelog_include: if include == "all" { (0..count).map(|i| format!("package-{i}")).collect() } else { vec!["package-1".to_owned()] },
                        ..Default::default()
                    });
                }
                let project = project(&req);
                let updater = Updater { project: &project, req: &req };
                let packages: Vec<Package> = (0..count)
                    .map(|i| fake_package::FakePackage::new(format!("package-{i}")).into())
                    .collect();
                let diffs: Vec<_> = packages.iter().map(|p| {
                    let mut diff = Diff::new(true);
                    diff.commits = (0..commits).map(|i| Commit::new(format!("{i:040x}"), format!("fix: update {} change {i}", p.name))).collect();
                    (p, diff)
                }).collect();
                if include != "none" {
                    assert_eq!(updater.perf_snapshot_commits(&diffs)["package-1"], diffs[1].1.commits);
                }
                measure(&format!("snapshots_{count}x{commits}_include_{include}"), || {
                    black_box(updater.perf_snapshot_commits(black_box(&diffs)));
                });
            }
        }
    }
}
'''
s = s.rstrip()
assert s.endswith('}')
s = s[:-1] + (Path(__file__).parent / 'client_bench.rs').read_text() + (Path(__file__).parent / 'group_control.rs').read_text() + '}\n'
p.write_text(s)
