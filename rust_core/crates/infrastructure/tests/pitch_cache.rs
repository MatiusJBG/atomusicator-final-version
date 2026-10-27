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
    assert_eq!(first, again);
    fs::write(&first[0].1, b"corrupt").unwrap();
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
    assert_ne!(repaired, other);
    let shifted = cache
        .get_or_render(
            &sources,
            PitchShiftSpec::try_new(3.0, true).unwrap(),
            "ffmpeg-sha-B/rubberband-v1",
            &cancel,
            &mut render,
        )
        .unwrap();
    assert_ne!(other, shifted);
    let formants = cache
        .get_or_render(
            &sources,
            PitchShiftSpec::try_new(3.0, false).unwrap(),
            "ffmpeg-sha-B/rubberband-v1",
            &cancel,
            &mut render,
        )
        .unwrap();
    assert_ne!(shifted, formants);
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
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
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
