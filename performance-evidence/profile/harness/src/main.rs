use std::{path::Path, process::Command, time::Instant};
use cargo_metadata::camino::Utf8Path;

mod copy_before;
mod copy_after;
mod fs_utils {
    use std::path::Path;
    use anyhow::Context;
    use cargo_metadata::camino::Utf8Path;
    pub fn strip_prefix(path: &Utf8Path, prefix: impl AsRef<Path>) -> anyhow::Result<&Utf8Path> {
        path.strip_prefix(prefix.as_ref())
            .with_context(|| format!("cannot strip prefix {:?} from {:?}", prefix.as_ref(), path))
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let source = Utf8Path::new(&args[1]);
    let variant = &args[2];
    let iterations: usize = args.get(3).map_or(20, |s| s.parse().unwrap());
    let git_status = |path: &Path| {
        let output = Command::new("git").current_dir(path).args(["status", "--porcelain"]).output().unwrap();
        assert!(output.status.success());
        output.stdout
    };
    let expected_status = git_status(source.as_std_path());
    let tracked = Command::new("git").current_dir(source).args(["ls-files", "-z"]).output().unwrap();
    assert!(tracked.status.success());
    let tracked = String::from_utf8(tracked.stdout).unwrap();
    let mut elapsed = std::time::Duration::ZERO;
    for iteration in 0..iterations + 2 {
        let destination = tempfile::tempdir().unwrap();
        let destination = Utf8Path::from_path(destination.path()).unwrap();
        let start = Instant::now();
        match variant.as_str() {
            "before" => copy_before::copy_dir(source, destination).unwrap(),
            "after" => copy_after::copy_dir(source, destination).unwrap(),
            _ => panic!("unknown variant"),
        }
        let duration = start.elapsed();
        if iteration >= 2 { elapsed += duration; }
        let copied = destination.join(source.file_name().unwrap());
        assert_eq!(git_status(copied.as_std_path()), expected_status);
        for relative in tracked.split_terminator('\0') {
            assert_eq!(std::fs::read(copied.join(relative)).unwrap(), std::fs::read(source.join(relative)).unwrap());
        }
        assert!(!copied.join("target/ignored.txt").exists());
    }
    println!("{variant},{iterations},{:.3}", elapsed.as_secs_f64() * 1e6 / iterations as f64);
}
