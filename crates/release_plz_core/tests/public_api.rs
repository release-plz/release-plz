use release_plz_core::{Publishable, fs_utils::Utf8TempDir, is_readme_updated};

struct DownstreamPackage;

// Existing downstream implementations only need to implement is_publishable.
impl Publishable for DownstreamPackage {
    fn is_publishable(&self) -> bool {
        true
    }
}

#[test]
fn existing_publishable_implementation_remains_compatible() {
    assert!(DownstreamPackage.is_publishable());
}

#[test]
fn public_readme_comparison_retains_its_original_signature_and_behavior() {
    let local = Utf8TempDir::new().unwrap();
    let registry = Utf8TempDir::new().unwrap();
    fs_err::create_dir(local.path().join("src")).unwrap();
    fs_err::write(local.path().join("src/lib.rs"), "").unwrap();
    fs_err::write(
        local.path().join("Cargo.toml"),
        "[package]\nname = \"example\"\nversion = \"0.1.0\"\nedition = \"2021\"\nreadme = \"notes.md\"\n",
    )
    .unwrap();
    fs_err::write(local.path().join("notes.md"), "readme").unwrap();
    // The public helper only needs the registry README, not registry metadata.
    fs_err::write(registry.path().join("README.md"), "readme").unwrap();
    assert!(!is_readme_updated("example", local.path(), registry.path()).unwrap());
    fs_err::write(local.path().join("notes.md"), "updated readme").unwrap();
    assert!(is_readme_updated("example", local.path(), registry.path()).unwrap());
}
