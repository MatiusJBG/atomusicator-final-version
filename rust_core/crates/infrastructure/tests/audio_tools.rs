use atomusicator_infrastructure::audio_tools::ExportOptions;

#[test]
fn export_options_reject_unsupported_formats_and_rates() {
    assert!(ExportOptions::new("wav", 44100, 2, 24, false).is_ok());
    assert!(ExportOptions::new("mp3", 48000, 1, 16, true).is_ok());
    assert!(ExportOptions::new("ogg", 44100, 2, 16, false).is_err());
    assert!(ExportOptions::new("wav", 0, 2, 16, false).is_err());
    assert!(ExportOptions::new("wav", 44100, 3, 16, false).is_err());
    assert!(ExportOptions::new("flac", 48000, 2, 32, false).is_err());
}

#[test]
#[ignore = "requires ATOMUSICATOR_FFMPEG pointing to a Rubber Band enabled binary"]
fn ffmpeg_pitch_preserves_duration_and_export_supports_all_legacy_formats() {
    use atomusicator_application::CancellationToken;
    use atomusicator_domain::{AudioBufferF32, AudioFormat, PitchShiftSpec};
    use atomusicator_infrastructure::{
        audio_io::wav_codec::{read_wav_f32, write_wav_f32},
        audio_tools::{AudioTools, TemporaryDirectory},
    };
    let tools = AudioTools::discover().unwrap();
    let directory = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let source = directory.0.join("source with spaces.wav");
    let original = AudioBufferF32::new(
        AudioFormat::new(44100, 2).unwrap(),
        (0..44100)
            .flat_map(|i| {
                let v = (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / 44100.0).sin() * 0.3;
                [v, v]
            })
            .collect(),
    )
    .unwrap();
    write_wav_f32(&source, &original).unwrap();
    for preserve in [false, true] {
        let target = directory.0.join(format!("pitch-{preserve}.wav"));
        tools
            .pitch(
                &source,
                &target,
                PitchShiftSpec::try_new(12.0, preserve).unwrap(),
                &CancellationToken::default(),
            )
            .unwrap();
        let shifted = read_wav_f32(&target).unwrap();
        assert_eq!(shifted.frames(), original.frames());
        let crossings = shifted
            .samples()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|f| f[0])
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|p| p[0] <= 0.0 && p[1] > 0.0)
            .count();
        assert!(
            (830..920).contains(&crossings),
            "expected octave up, got {crossings} crossings"
        );
    }
    for format in ["wav", "flac", "mp3"] {
        let target = directory.0.join(format!("export.{format}"));
        tools
            .export(
                &source,
                &target,
                &ExportOptions::new(format, 48000, 1, 24, true).unwrap(),
                &CancellationToken::default(),
            )
            .unwrap();
        assert!(std::fs::metadata(&target).unwrap().len() > 1000);
        let decoded = tools
            .decode_for_separation(&target, &CancellationToken::default())
            .unwrap();
        assert_eq!(decoded.format(), original.format());
        assert!(decoded.frames().abs_diff(original.frames()) < 2205);
    }
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(
        tools
            .pitch(
                &source,
                &directory.0.join("cancelled.wav"),
                PitchShiftSpec::try_new(2.0, false).unwrap(),
                &cancel
            )
            .is_err()
    );
    assert!(!directory.0.join("cancelled.wav").exists());
    assert_eq!(read_wav_f32(&source).unwrap(), original);
}
