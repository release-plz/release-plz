from pathlib import Path
p=Path('/workspace/perf-worktrees/core-update/crates/release_plz_core/src/release_order.rs')
s=p.read_text()
s=s.replace('release_order_inner(&packages_by_name, p, &mut order, &mut passed)?;', 'release_order_inner(&mut packages_by_name, p, &mut order, &mut passed)?;')
s=s.replace("    packages: &HashMap<&str, &'a Package>,", "    packages: &mut HashMap<&str, &'a Package>,")
s=s.replace('    if is_package_in(pkg, order) {', '    // Completed packages have been removed from the map.\n    if !packages.contains_key(pkg.name.as_str()) {')
s=s.replace('    order.push(pkg);', '    order.push(pkg);\n    packages.remove(pkg.name.as_str());')
needle='    /// ┌──┐\n    /// │  ▼\n    /// A  B (dev dependency)\n    /// ▲  │\n    /// └──┘\n    #[test]\n    fn two_packages_dev_cycle_with_package_in_features_is_detected()'
new='''    /// Shared dependencies are emitted once, preserving traversal order.
    #[test]
    fn shared_dependency_is_released_once() {
        let pkgs = [
            &pkg("a", &[dep("b"), dep("c")]),
            &pkg("b", &[dep("d")]),
            &pkg("c", &[dep("d")]),
            &pkg("d", &[]),
        ];
        assert_eq!(order(&pkgs), ["d", "b", "c", "a"]);
    }

'''+needle
assert needle in s
s=s.replace(needle,new)
p.write_text(s)
