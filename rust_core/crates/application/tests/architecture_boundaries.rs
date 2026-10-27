use std::{fs, path::PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn application_depends_only_on_the_domain_crate() {
    let manifest = fs::read_to_string(manifest_dir().join("Cargo.toml")).unwrap();
    let dependencies = manifest.split("[dependencies]").nth(1).unwrap();
    assert!(dependencies.contains("atomusicator-domain = { path = \"../domain\" }"));
    assert_eq!(
        dependencies
            .lines()
            .filter(|line| line.contains('=') && !line.trim_start().starts_with('#'))
            .count(),
        1
    );
}

#[test]
fn domain_has_no_third_party_dependencies() {
    let manifest = fs::read_to_string(manifest_dir().join("../domain/Cargo.toml")).unwrap();
    let dependencies = manifest.split("[dependencies]").nth(1).unwrap();
    assert!(dependencies.trim().is_empty());
}
