use std::{hint::black_box, time::{Duration, Instant}};
use cargo_metadata::Package;
use serde_json::json;
#[path = "../order_before.rs"] mod order_before;
#[path = "../order_after.rs"] mod order_after;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let count: usize = args[1].parse().unwrap();
    let variant = &args[2];
    let packages: Vec<Package> = (0..count).map(|i| {
        let mut dependencies: Vec<_> = (0..20).map(|d| dependency(&format!("registry-dependency-{d}"))).collect();
        if i > 0 { dependencies.push(dependency(&format!("workspace-package-{}", i-1))); }
        serde_json::from_value(json!({
            "name": format!("workspace-package-{i}"), "version": "1.0.0",
            "id": format!("workspace-package-{i}"), "dependencies": dependencies,
            "features": {}, "manifest_path": format!("/crates/package-{i}/Cargo.toml"), "targets": []
        })).unwrap()
    }).collect();
    // Analyze dependents before dependencies so the recursive traversal is exercised.
    let refs: Vec<_> = packages.iter().rev().collect();
    let function = match variant.as_str() {
        "before" => order_before::release_order,
        "after" => order_after::release_order,
        _ => panic!("unknown variant"),
    };
    let result = function(&refs).unwrap();
    assert_eq!(result.len(), count);
    assert!(result.iter().zip(&packages).all(|(actual, expected)| actual.name == expected.name));
    let mut iterations = 0;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        black_box(function(black_box(&refs)).unwrap());
        iterations += 1;
    }
    println!("{count},{variant},{iterations},{:.3}", start.elapsed().as_secs_f64()*1e9 / iterations as f64);
}
fn dependency(name: &str) -> serde_json::Value {
    json!({"name":name,"req":"1.0.0","kind":"normal","optional":false,"uses_default_features":true,"features":[]})
}
