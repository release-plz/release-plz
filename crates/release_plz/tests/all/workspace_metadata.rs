use cargo_metadata::camino::Utf8Path;

#[test]
fn workspace_package_ignores_nonmembers_with_the_same_name() {
    let fixture = Utf8Path::new("../../tests/fixtures/set-version-in-workspace");
    let directory = release_plz_core::copy_to_temp_dir(fixture).unwrap();
    let manifest = directory.path().join("set-version-in-workspace/Cargo.toml");
    let mut metadata = cargo_utils::get_manifest_metadata(&manifest).unwrap();
    let member = cargo_utils::workspace_package(&metadata, "one")
        .unwrap()
        .clone();
    let mut dependency = member.clone();
    dependency.id.repr = "registry+https://github.com/rust-lang/crates.io-index#one@0.1.0".into();
    metadata.packages.insert(0, dependency.clone());
    dependency.name = cargo_metadata::PackageName::new("external".to_owned());
    dependency.id.repr =
        "registry+https://github.com/rust-lang/crates.io-index#external@0.1.0".into();
    metadata.packages.push(dependency);

    assert_eq!(
        cargo_utils::workspace_package(&metadata, "one").unwrap().id,
        member.id
    );
    assert!(cargo_utils::workspace_package(&metadata, "external").is_err());
    assert!(cargo_utils::workspace_package(&metadata, "missing").is_err());

    metadata.workspace_members.retain(|id| id != &member.id);
    assert!(cargo_utils::workspace_package(&metadata, "one").is_err());
}
