//! Flutter-facing API. Heavy work is exposed as asynchronous bridge calls.

#[flutter_rust_bridge::frb]
pub fn application_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[flutter_rust_bridge::frb]
pub fn supported_stems() -> Vec<String> {
    [
        "Lead Vocal",
        "Backing Vocals",
        "Drums",
        "Bass",
        "Instrumental",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
