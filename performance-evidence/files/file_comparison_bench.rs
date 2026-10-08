use std::{hint::black_box, time::{Duration, Instant}};
use cargo_metadata::camino::Utf8Path;
use release_plz_core::are_packages_equal;

fn main() {
    println!("scenario,iterations,ns_per_comparison");
    for (name, count, size, change) in [
        ("equal_1k", 1, 1024, "equal"),
        ("equal_100x1k", 100, 1024, "equal"),
        ("equal_64k", 1, 64 * 1024, "equal"),
        ("equal_1m", 1, 1024 * 1024, "equal"),
        ("first_byte_1m", 1, 1024 * 1024, "first"),
        ("last_byte_1m", 1, 1024 * 1024, "last"),
        ("truncated_1m", 1, 1024 * 1024, "truncate"),
        ("equal_8m", 1, 8 * 1024 * 1024, "equal"),
        ("first_byte_8m", 1, 8 * 1024 * 1024, "first"),
        ("last_byte_8m", 1, 8 * 1024 * 1024, "last"),
        ("truncated_8m", 1, 8 * 1024 * 1024, "truncate"),
    ] {
        let fixture = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(fixture.path()).unwrap();
        let local = root.join("local");
        let registry = root.join("registry");
        let manifest = "[package]\nname = \"bench\"\nversion = \"0.1.0\"\nedition = \"2024\"\n";
        for dir in [&local, &registry] {
            fs_err::create_dir(dir).unwrap();
            fs_err::write(dir.join("Cargo.toml"), manifest).unwrap();
            fs_err::write(dir.join("Cargo.toml.orig"), manifest).unwrap();
        }
        let original: Vec<_> = (0..size).map(|i| ((i * 31) ^ (i >> 4)) as u8).collect();
        let mut changed = original.clone();
        match change {
            "first" => changed[0] ^= 1,
            "last" => changed[size - 1] ^= 1,
            "truncate" => { changed.pop(); },
            _ => {},
        }
        for i in 0..count {
            let filename = format!("source{i:03}.rs");
            fs_err::write(local.join(&filename), &original).unwrap();
            fs_err::write(registry.join(&filename), &changed).unwrap();
        }
        let expected = change == "equal";
        for _ in 0..10 {
            assert_eq!(are_packages_equal(&local, &registry).unwrap(), expected);
        }
        let start = Instant::now();
        let mut iterations = 0_u64;
        while start.elapsed() < Duration::from_millis(500) {
            let result = are_packages_equal(black_box(&local), black_box(&registry)).unwrap();
            assert_eq!(black_box(result), expected);
            iterations += 1;
        }
        let elapsed = start.elapsed();
        println!("{name},{iterations},{:.2}", elapsed.as_nanos() as f64 / iterations as f64);
    }
}
