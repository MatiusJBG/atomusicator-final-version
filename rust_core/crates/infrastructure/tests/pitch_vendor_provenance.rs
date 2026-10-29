use sha2::{Digest, Sha256};

#[test]
fn embedded_pitch_sources_match_the_pinned_upstream_bytes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("native/vendor");
    let provenance: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("PROVENANCE.json")).unwrap()).unwrap();
    for entry in provenance["files"].as_array().unwrap() {
        let relative = entry["path"].as_str().unwrap();
        let bytes = std::fs::read(root.join(relative)).unwrap();
        let actual = format!("{:x}", Sha256::digest(bytes));
        assert_eq!(actual, entry["sha256"].as_str().unwrap(), "{relative}");
    }
}
