//! Flutter-facing API. Heavy work is exposed as asynchronous bridge calls.

use atomusicator_domain::StemType;

/// Flutter-facing description of one persisted stem track.
#[derive(Debug, Clone)]
pub struct UiStemTrackDto {
    pub stem_type: String,
    pub file_path: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_seconds: f64,
}

/// Health check for the Rust core and its canonical five-stem contract.
#[flutter_rust_bridge::frb]
pub fn get_engine_version() -> String {
    format!(
        "Atomusicator Native Core v{} (Stems: {})",
        env!("CARGO_PKG_VERSION"),
        StemType::ALL.len()
    )
}

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

#[cfg(test)]
mod tests {
    #[test]
    fn engine_version_reports_native_core_and_stem_count() {
        assert_eq!(
            super::get_engine_version(),
            "Atomusicator Native Core v0.6.0 (Stems: 5)"
        );
    }
}
