use std::{
    process::{Command, Output},
    time::{Duration, Instant},
};

use cargo_metadata::camino::Utf8Path;
use criterion::{Criterion, SamplingMode, criterion_group, criterion_main};
use git_cmd::Repo;
use release_plz_core::fs_utils::Utf8TempDir;

fn update(c: &mut Criterion) {
    let mut group = c.benchmark_group("update");
    group
        .sample_size(10)
        .sampling_mode(SamplingMode::Flat)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(5));

    for (name, packages, commits, dependency_chain) in [
        ("single_noop", 1, 0, false),
        ("single_change", 1, 1, false),
        ("history_100", 1, 100, false),
        ("workspace_10_noop", 10, 0, false),
        ("workspace_10_one_change", 10, 1, false),
        ("workspace_10_dependency_chain", 10, 1, true),
    ] {
        let mut fixture = None;
        group.bench_function(name, |b| {
            // Construct only selected fixtures and retain them across warmup and samples.
            let fixture =
                fixture.get_or_insert_with(|| Fixture::new(packages, commits, dependency_chain));
            b.iter_custom(|iterations| {
                let mut elapsed = Duration::ZERO;
                for _ in 0..iterations {
                    elapsed += fixture.measure_update(packages, commits, dependency_chain);
                }
                elapsed
            });
        });
    }
    group.finish();
}

struct Fixture {
    directory: Utf8TempDir,
    repo: Repo,
}

impl Fixture {
    fn new(packages: usize, commits: usize, dependency_chain: bool) -> Self {
        let directory = Utf8TempDir::new().unwrap();
        for name in ["current", "released"] {
            let root = directory.path().join(name);
            fs_err::create_dir(&root).unwrap();
            fs_err::write(
                root.join("Cargo.toml"),
                "[workspace]\nmembers = [\"packages/*\"]\nresolver = \"3\"\n",
            )
            .unwrap();
            for index in 0..packages {
                let package = root.join(format!("packages/package-{index}"));
                fs_err::create_dir_all(package.join("src")).unwrap();
                let mut manifest = format!(
                    "[package]\nname = \"package-{index}\"\nversion = \"1.0.0\"\nedition = \"2024\"\n"
                );
                if dependency_chain && index > 0 {
                    let previous = index - 1;
                    manifest.push_str(&format!(
                        "[dependencies]\npackage-{previous} = {{ path = \"../package-{previous}\", version = \"=1.0.0\" }}\n"
                    ));
                }
                fs_err::write(package.join("Cargo.toml"), manifest).unwrap();
                fs_err::write(package.join("src/lib.rs"), "// Initial release\n").unwrap();
            }
            let output = Command::new("cargo")
                .current_dir(&root)
                .args(["generate-lockfile", "--offline"])
                .output()
                .unwrap();
            assert_success(&output);
        }

        let current = directory.path().join("current");
        // Keep Git checkout from changing source bytes on machines using core.autocrlf.
        fs_err::write(current.join(".gitattributes"), "* -text\n").unwrap();
        fs_err::write(
            current.join("release-plz.toml"),
            "[workspace]\nsemver_check = false\n",
        )
        .unwrap();
        // An explicit template prevents a user's global git-cliff config changing the workload.
        fs_err::write(
            current.join("cliff.toml"),
            r##"[changelog]
header = "# Changelog\n"
body = """
## [{{ version }}] - {{ timestamp | date(format="%Y-%m-%d") }}
{% for commit in commits %}- {{ commit.message }}
{% endfor %}
"""
[git]
conventional_commits = true
filter_unconventional = false
"##,
        )
        .unwrap();
        let repo = Repo::init(&current);
        let mut source = String::from("// Initial release\n");
        for index in 0..commits {
            source.push_str(&format!("// Fix {index}\n"));
            fs_err::write(current.join("packages/package-0/src/lib.rs"), &source).unwrap();
            repo.add_all_and_commit(&format!("fix: change {index}"))
                .unwrap();
        }
        Self { directory, repo }
    }

    fn measure_update(&self, packages: usize, commits: usize, dependency_chain: bool) -> Duration {
        // Reset only our disposable repository. Both tracked updates and newly created
        // changelogs must be removed, or subsequent iterations measure different work.
        self.repo.git(&["reset", "--hard", "HEAD"]).unwrap();
        self.repo.git(&["clean", "-fd"]).unwrap();

        let mut command = Command::new(env!("CARGO_BIN_EXE_release-plz"));
        command
            .current_dir(self.repo.directory())
            .env("CARGO_NET_OFFLINE", "true")
            .env("CARGO_TARGET_DIR", self.directory.path().join("target"))
            .env("RELEASE_PLZ_NO_ANSI", "1")
            .env("RELEASE_PLZ_LOG", "error")
            .env_remove("GIT_TOKEN")
            .args([
                "update",
                "--release-date",
                "2025-01-01",
                "--changelog-config",
                "cliff.toml",
                "--registry-manifest-path",
            ])
            .arg(self.directory.path().join("released/Cargo.toml"));

        let start = Instant::now();
        let output = command.output().unwrap();
        let elapsed = start.elapsed();

        // Validate every run after stopping the clock: failures and skipped work
        // must never be reported as performance improvements.
        assert_success(&output);
        for index in 0..packages {
            let package = self
                .repo
                .directory()
                .join(format!("packages/package-{index}"));
            let changed = commits > 0 && (index == 0 || dependency_chain);
            let expected = if changed { "1.0.1" } else { "1.0.0" };
            assert_eq!(package_version(&package), expected);
            let changelog = package.join("CHANGELOG.md");
            assert_eq!(changelog.exists(), changed);
            if changed {
                let contents = fs_err::read_to_string(changelog).unwrap();
                assert!(contents.contains("1.0.1"));
                if index == 0 {
                    for commit in 0..commits {
                        assert!(contents.contains(&format!("change {commit}\n")));
                    }
                }
            }
        }
        if commits == 0 {
            self.repo.is_clean().unwrap();
        }
        elapsed
    }
}

fn package_version(package: &Utf8Path) -> String {
    let manifest = fs_err::read_to_string(package.join("Cargo.toml")).unwrap();
    let manifest: toml::Value = toml::from_str(&manifest).unwrap();
    manifest["package"]["version"].as_str().unwrap().to_owned()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

criterion_group!(benches, update);
criterion_main!(benches);
