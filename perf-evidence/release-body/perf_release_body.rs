#[allow(dead_code)]
#[path = "../src/tera.rs"]
mod tera;
use std::{hint::black_box, time::{Duration, Instant}};
use release_plz_core::Remote;

fn main() {
    let remote = Remote { owner: "owner".into(), repo: "repo".into(), link: "https://example.com/owner/repo".into(), contributors: vec![] };
    let small = "## Changes\n\n- Fix release automation (#123).\n".to_owned();
    let large = small.repeat(400);
    for (name, changelog, template) in [
        ("default_small", small.as_str(), None),
        ("default_17kb", large.as_str(), None),
        ("custom_small", small.as_str(), Some("{{ package }} {{ version }}: {{ changelog }}")),
    ] {
        let expected = if template.is_some() { format!("example 1.2.3: {changelog}") } else { changelog.to_owned() };
        assert_eq!(tera::release_body_from_template("example", "1.2.3", changelog, &remote, template).unwrap(), expected);
        let run = |iterations: usize| {
            let start = Instant::now();
            for _ in 0..iterations {
                black_box(tera::release_body_from_template(black_box("example"), black_box("1.2.3"), black_box(changelog), black_box(&remote), black_box(template)).unwrap());
            }
            start.elapsed()
        };
        let mut iterations = 100;
        let calibration = loop {
            let elapsed = run(iterations);
            if elapsed >= Duration::from_millis(50) { break elapsed; }
            iterations *= 2;
        };
        iterations = (iterations as f64 * 0.5 / calibration.as_secs_f64()) as usize;
        let elapsed = run(iterations);
        println!("{name} bytes={} iterations={iterations} elapsed_ns={} ns_per_iter={:.3}", changelog.len(), elapsed.as_nanos(), elapsed.as_nanos() as f64 / iterations as f64);
    }
}
