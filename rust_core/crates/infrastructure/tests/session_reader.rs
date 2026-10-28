use atomusicator_application::CancellationToken;
use atomusicator_domain::{AudioBufferF32, AudioFormat, StemType};
use atomusicator_infrastructure::{
    audio_io::wav_codec::write_wav_f32,
    audio_tools::TemporaryDirectory,
    session_reader::{read_session_files, stem_key},
};
use std::fs;

#[test]
fn session_reader_rejects_partial_duplicate_and_corrupt_bundles_and_accepts_legacy_maps() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let mut entries = Vec::new();
    let mut legacy = serde_json::Map::new();
    for kind in StemType::ALL {
        let path = format!("{}.wav", stem_key(kind));
        write_wav_f32(
            &root.0.join(&path),
            &AudioBufferF32::new(AudioFormat::new(44100, 2).unwrap(), vec![0.2; 64]).unwrap(),
        )
        .unwrap();
        entries.push(serde_json::json!({"kind":stem_key(kind),"path":path}));
        legacy.insert(format!("{kind:?}"), serde_json::json!({"path":path}));
    }
    let value = serde_json::json!({"session_id":"verified","file_name":"example","separation":{"state":"complete","stems":entries}});
    let manifest = root.0.join("session.json");
    let cancel = CancellationToken::default();
    for fault in [
        "none",
        "partial",
        "duplicate",
        "extra",
        "checksum",
        "legacy",
    ] {
        let mut record = value.clone();
        match fault {
            "partial" => record["separation"]["state"] = "running".into(),
            "duplicate" => record["separation"]["stems"][1]["kind"] = "lead_vocal".into(),
            "extra" => record["separation"]["stems"]
                .as_array_mut()
                .unwrap()
                .push(entries[0].clone()),
            "checksum" => record["separation"]["stems"][0]["sha256"] = "0".repeat(64).into(),
            "legacy" => record["separation"]["stems"] = serde_json::Value::Object(legacy.clone()),
            _ => {}
        }
        fs::write(&manifest, serde_json::to_vec(&record).unwrap()).unwrap();
        assert_eq!(
            read_session_files(&root.0, &cancel).is_ok(),
            matches!(fault, "none" | "legacy"),
            "{fault}"
        );
    }
}
