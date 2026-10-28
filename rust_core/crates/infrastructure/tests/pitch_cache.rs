use atomusicator_application::CancellationToken;
use atomusicator_domain::{AudioBufferF32, AudioFormat, PitchShiftSpec, StemType};
use atomusicator_infrastructure::{
    audio_io::wav_codec::write_wav_f32, audio_tools::TemporaryDirectory,
    pitch_cache::PitchRenderCache,
};
use std::{fs, path::PathBuf};

fn originals(root: &std::path::Path) -> Vec<(StemType, PathBuf)> {
    StemType::ALL
        .into_iter()
        .map(|kind| {
            let path = root.join(format!("{kind:?}.wav"));
            write_wav_f32(
                &path,
                &AudioBufferF32::new(AudioFormat::new(44100, 2).unwrap(), vec![0.1; 100]).unwrap(),
            )
            .unwrap();
            (kind, path)
        })
        .collect()
}
#[test]
fn pruning_retains_live_pitch_batches_and_never_deletes_originals_or_unowned_folders() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let sources = originals(&root.0);
    let render_root = root.0.join("renders");
    let cache = PitchRenderCache::new(render_root.clone());
    let batch = cache
        .get_or_render(
            &sources,
            PitchShiftSpec::try_new(1.0, false).unwrap(),
            "runtime",
            &CancellationToken::default(),
            |source, target| {
                fs::copy(source, target).unwrap();
                Ok(())
            },
        )
        .unwrap();
    let rendered = batch.paths[0].1.clone();
    let directory = rendered.parent().unwrap().to_path_buf();
    let unrelated = render_root.join(".invalid-user-notes");
    fs::create_dir(&unrelated).unwrap();
    fs::write(unrelated.join(".cache-key"), "a".repeat(64)).unwrap();
    fs::write(unrelated.join("keep.txt"), b"user file").unwrap();
    cache.prune(0).unwrap();
    assert!(rendered.exists(), "a reader still owns this batch");
    drop(batch);
    cache.prune(0).unwrap();
    assert!(!directory.exists(), "unleased cache can be evicted");
    assert!(unrelated.join("keep.txt").exists());
    assert!(sources.iter().all(|(_, path)| path.exists()));
}
#[test]
fn renders_are_reused_only_when_every_identity_and_output_hash_matches() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let sources = originals(&root.0);
    let cache = PitchRenderCache::new(root.0.join("renders"));
    let cancel = CancellationToken::default();
    let spec = PitchShiftSpec::try_new(2.0, true).unwrap();
    let mut calls = 0;
    let mut render = |source: &std::path::Path, target: &std::path::Path| {
        calls += 1;
        fs::copy(source, target).unwrap();
        Ok(())
    };
    let first = cache
        .get_or_render(
            &sources,
            spec,
            "ffmpeg-sha-A/rubberband-v1",
            &cancel,
            &mut render,
        )
        .unwrap();
    let again = cache
        .get_or_render(
            &sources,
            spec,
            "ffmpeg-sha-A/rubberband-v1",
            &cancel,
            &mut render,
        )
        .unwrap();
    assert_eq!(first.paths, again.paths);
    fs::write(&first.paths[0].1, b"corrupt").unwrap();
    let repaired = cache
        .get_or_render(
            &sources,
            spec,
            "ffmpeg-sha-A/rubberband-v1",
            &cancel,
            &mut render,
        )
        .unwrap();
    assert!(
        repaired
            .paths
            .iter()
            .all(|(_, path)| fs::metadata(path).unwrap().len() > 7)
    );
    let other = cache
        .get_or_render(
            &sources,
            spec,
            "ffmpeg-sha-B/rubberband-v1",
            &cancel,
            &mut render,
        )
        .unwrap();
    assert_ne!(repaired.paths, other.paths);
    let shifted = cache
        .get_or_render(
            &sources,
            PitchShiftSpec::try_new(3.0, true).unwrap(),
            "ffmpeg-sha-B/rubberband-v1",
            &cancel,
            &mut render,
        )
        .unwrap();
    assert_ne!(other.paths, shifted.paths);
    let formants = cache
        .get_or_render(
            &sources,
            PitchShiftSpec::try_new(3.0, false).unwrap(),
            "ffmpeg-sha-B/rubberband-v1",
            &cancel,
            &mut render,
        )
        .unwrap();
    assert_ne!(shifted.paths, formants.paths);
    assert_eq!(calls, 25);
}

#[test]
fn cancellation_never_publishes_a_partial_batch_or_changes_originals() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let sources = originals(&root.0);
    let original = fs::read(&sources[0].1).unwrap();
    let directory = root.0.join("renders");
    let cache = PitchRenderCache::new(directory.clone());
    let cancel = CancellationToken::default();
    let result = cache.get_or_render(
        &sources,
        PitchShiftSpec::try_new(2.0, false).unwrap(),
        "runtime",
        &cancel,
        |source, target| {
            fs::copy(source, target).unwrap();
            cancel.cancel();
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(fs::read(&sources[0].1).unwrap(), original);
    assert_eq!(
        fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap())
            .filter(|entry| entry.file_type().unwrap().is_dir())
            .count(),
        0,
        "no partial batch is published; coordination lock files are metadata"
    );
}

#[test]
fn changing_an_original_during_render_does_not_publish_a_stale_cache_key() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let sources = originals(&root.0);
    let cache = PitchRenderCache::new(root.0.join("renders"));
    let cancel = CancellationToken::default();
    let mut first = true;
    let result = cache.get_or_render(
        &sources,
        PitchShiftSpec::try_new(2.0, true).unwrap(),
        "runtime",
        &cancel,
        |source, target| {
            fs::copy(source, target).unwrap();
            if first {
                first = false;
                write_wav_f32(
                    source,
                    &AudioBufferF32::new(AudioFormat::new(44100, 2).unwrap(), vec![0.2; 100])
                        .unwrap(),
                )
                .unwrap();
            }
            Ok(())
        },
    );
    assert!(result.is_err());
}
