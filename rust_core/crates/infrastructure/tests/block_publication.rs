use atomusicator_application::{CancellationToken, ports::AudioBlockStore};
use atomusicator_domain::{AudioAsset, AudioAssetId, AudioAssetMetadata, AudioFormat, StemType};
use atomusicator_infrastructure::{
    audio_tools::TemporaryDirectory,
    block_workspace::{PublicationHooks, publish_blocks},
    storage::float_spool::FloatSpoolStore,
};
use std::collections::BTreeMap;

#[test]
fn only_the_commit_boundary_can_publish_a_complete_bundle() {
    for mode in ["cancel-before", "commit-fails", "cancel-after", "success"] {
        let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let projects = root.0.join("projects with spaces ñ");
        std::fs::create_dir(&projects).unwrap();
        let store = FloatSpoolStore::new(root.0.join("spools"));
        let mut writer = store
            .create(
                "source",
                AudioAssetMetadata {
                    format: AudioFormat::new(44100, 2).unwrap(),
                    frame_count: 4,
                },
            )
            .unwrap();
        writer.write(&[0.1; 8]).unwrap();
        let source = writer.finish().unwrap();
        let stems: BTreeMap<_, _> = StemType::ALL
            .into_iter()
            .map(|k| (k, source.clone()))
            .collect();
        let asset = AudioAsset {
            id: AudioAssetId::new("source").unwrap(),
            path: root.0.join("original.flac"),
            display_name: "original".into(),
        };
        let cancel = CancellationToken::default();
        let result = publish_blocks(
            &projects,
            "attempt-1",
            &asset,
            &stems,
            serde_json::json!({}),
            &cancel,
            PublicationHooks {
                staged: |path: &std::path::Path| {
                    assert_eq!(
                        std::fs::read_to_string(path.join(".job-owner")).unwrap(),
                        "attempt-1"
                    );
                    if mode == "cancel-before" {
                        cancel.cancel();
                    }
                    Ok(())
                },
                commit: || {
                    assert_ne!(mode, "cancel-before", "cancel must prevent commit");
                    if mode == "commit-fails" {
                        return Err("journal unavailable".into());
                    }
                    if mode == "cancel-after" {
                        cancel.cancel();
                    }
                    Ok(())
                },
            },
        );
        let should_publish = matches!(mode, "success" | "cancel-after");
        assert_eq!(result.is_ok(), should_publish, "{mode}");
        let entries: Vec<_> = std::fs::read_dir(&projects).unwrap().collect();
        assert_eq!(
            entries.len(),
            usize::from(should_publish),
            "no abandoned staging: {mode}"
        );
        if should_publish {
            let bundle = result.unwrap();
            assert!(bundle.directory.join("session.json").is_file());
            assert_eq!(
                std::fs::read_dir(bundle.directory.join("outputs"))
                    .unwrap()
                    .count(),
                5
            );
        }
    }
}
