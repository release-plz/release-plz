use std::{hint::black_box, time::Instant};

use cargo_utils::{DepKind, LocalManifest};

fn fixture(targets: usize) -> LocalManifest {
    let mut input = String::from(
        "[package]\nname = 'bench'\nversion = '1.0.0'\n\
         [dependencies]\nnormal = '1'\n\
         [dev-dependencies]\ndev = '1'\n\
         [build-dependencies]\nbuild = '1'\n\
         [workspace.dependencies]\nworkspace_only = '1'\n",
    );
    for index in 0..targets {
        for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
            input.push_str(&format!(
                "[target.'cfg(target_os = \"target-{index}\")'.{kind}]\ntarget_dep_{index} = '1'\n"
            ));
        }
    }
    LocalManifest {
        path: "/unused/Cargo.toml".into(),
        manifest: input.parse().unwrap(),
    }
}

fn verify(manifest: &LocalManifest, targets: usize) {
    let actual: Vec<_> = manifest
        .get_package_dependency_tables()
        .flat_map(|(kind, table)| table.iter().map(move |(name, _)| (kind, name.to_owned())))
        .collect();
    let mut expected = vec![
        (DepKind::Normal, "normal".to_owned()),
        (DepKind::Development, "dev".to_owned()),
        (DepKind::Build, "build".to_owned()),
    ];
    for index in 0..targets {
        for kind in [DepKind::Normal, DepKind::Development, DepKind::Build] {
            expected.push((kind, format!("target_dep_{index}")));
        }
    }
    assert_eq!(actual, expected);
    assert_eq!(manifest.get_dependency_tables().count(), expected.len() + 1);
}

fn visit_all(manifest: &LocalManifest) -> usize {
    manifest
        .get_package_dependency_tables()
        .map(|(kind, table)| table.len() + kind as usize)
        .sum()
}

fn find_first_target(manifest: &LocalManifest) -> usize {
    manifest
        .get_package_dependency_tables()
        .find(|(_, table)| table.contains_key(black_box("target_dep_0")))
        .map(|(kind, table)| table.len() + kind as usize)
        .unwrap()
}

fn measure(
    name: &str,
    manifest: &LocalManifest,
    iterations: usize,
    f: fn(&LocalManifest) -> usize,
) {
    for _ in 0..10_000 {
        black_box(f(black_box(manifest)));
    }
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(f(black_box(manifest)));
    }
    let elapsed = start.elapsed();
    println!(
        "{name}: {:.3} ns/iteration; iterations={iterations}; elapsed={:.6}s",
        elapsed.as_secs_f64() * 1e9 / iterations as f64,
        elapsed.as_secs_f64(),
    );
}

fn main() {
    let standard = fixture(0);
    let many_targets = fixture(20);
    verify(&standard, 0);
    verify(&many_targets, 20);
    assert_eq!(visit_all(&standard), 6);
    assert_eq!(visit_all(&many_targets), 126);
    assert_eq!(find_first_target(&many_targets), 1);
    measure("standard_3_tables/all", &standard, 10_000_000, visit_all);
    measure("target_63_tables/all", &many_targets, 1_000_000, visit_all);
    measure(
        "target_63_tables/first_match",
        &many_targets,
        1_000_000,
        find_first_target,
    );
}
