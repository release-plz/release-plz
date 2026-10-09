#![allow(dead_code)]
use std::{hint::black_box, time::{Duration, Instant}};
const NO_COMMIT_ID: &str = "0000000";
mod changelog_parser;
mod before { include!("before.rs"); include!("helpers.rs"); }
mod after_remote { include!("after-remote.rs"); include!("helpers.rs"); }
mod after_defaults { include!("after-defaults.rs"); include!("helpers.rs"); }

fn median(values: &mut [f64]) -> f64 { values.sort_by(f64::total_cmp); values[values.len()/2] }
fn batch<T>(op: &mut impl FnMut() -> T, duration: Duration) -> f64 {
    let start = Instant::now();
    let mut iterations = 0;
    while start.elapsed() < duration { for _ in 0..10 { black_box(op()); } iterations += 10; }
    start.elapsed().as_nanos() as f64 / iterations as f64
}
fn measure_pair<T,U>(name: &str, mut before: impl FnMut() -> T, mut after: impl FnMut() -> U) {
    batch(&mut before, Duration::from_millis(100));
    batch(&mut after, Duration::from_millis(100));
    let mut old = Vec::new(); let mut new = Vec::new();
    for i in 0..21 {
        let (a,b) = if i % 2 == 0 {
            (batch(&mut before, Duration::from_millis(100)), batch(&mut after, Duration::from_millis(100)))
        } else {
            let b = batch(&mut after, Duration::from_millis(100));
            (batch(&mut before, Duration::from_millis(100)), b)
        };
        println!("sample {name} {i} before_ns={a:.3} after_ns={b:.3}");
        old.push(a); new.push(b);
    }
    let a = median(&mut old); let b = median(&mut new);
    println!("RESULT {name} before_ns={a:.3} after_ns={b:.3} reduction_percent={:.3}", (1.0-b/a)*100.0);
}
fn main() {
    for count in [1,100,1000] {
        assert_eq!(before::remote_output(count), after_remote::remote_output(count));
    }
    for with_pr in [false,true] {
        assert_eq!(serde_json::to_value(before::perf_config(with_pr)).unwrap(), serde_json::to_value(after_defaults::perf_config(with_pr)).unwrap());
        for prepend in [false,true] {
            assert_eq!(before::generate_fixture(0,prepend,with_pr)(), after_defaults::generate_fixture(0,prepend,with_pr)());
        }
    }
    println!("VALIDATION all remote rendering and default config/render/prepend checks passed");
    for count in [10,100,1000] {
        measure_pair(&format!("remote_context_{count}"), before::context_fixture(count), after_remote::context_fixture(count));
        measure_pair(&format!("remote_generate_{count}"), before::generate_fixture(count,false,false), after_remote::generate_fixture(count,false,false));
    }
    measure_pair("default_config", || before::perf_config(false), || after_defaults::perf_config(false));
    measure_pair("default_generate", before::generate_fixture(0,false,false), after_defaults::generate_fixture(0,false,false));
    measure_pair("default_prepend_pr", before::generate_fixture(0,true,true), after_defaults::generate_fixture(0,true,true));
}
