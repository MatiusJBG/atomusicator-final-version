use atomusicator_infrastructure::audio_tools::ExportOptions;

#[test]
fn export_options_reject_unsupported_formats_and_rates() {
    assert!(ExportOptions::new("wav", 44100, 2, 24, false).is_ok());
    assert!(ExportOptions::new("wav", 44100, 2, 32, false).is_ok());
    assert!(ExportOptions::new("mp3", 48000, 1, 16, true).is_ok());
    assert!(ExportOptions::new("ogg", 44100, 2, 16, false).is_err());
    assert!(ExportOptions::new("wav", 0, 2, 16, false).is_err());
    assert!(ExportOptions::new("wav", 44100, 3, 16, false).is_err());
    assert!(ExportOptions::new("flac", 48000, 2, 32, false).is_err());
    assert!(ExportOptions::new("mp3", 48000, 2, 32, false).is_err());
}

#[test]
#[ignore = "requires ATOMUSICATOR_FFMPEG pointing to a Rubber Band enabled binary and its sibling FFprobe"]
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
    for (preserve, semitones) in [false, true].into_iter().flat_map(|preserve| {
        [-12.0, -7.0, -1.0, 0.0, 1.0, 7.0, 12.0]
            .into_iter()
            .map(move |s| (preserve, s))
    }) {
        let target = directory
            .0
            .join(format!("pitch-{preserve}-{semitones}.wav"));
        tools
            .pitch(
                &source,
                &target,
                PitchShiftSpec::try_new(semitones, preserve).unwrap(),
                &CancellationToken::default(),
            )
            .unwrap();
        let shifted = read_wav_f32(&target).unwrap();
        assert_eq!(shifted.frames(), original.frames());
        let crossings = shifted.samples().as_chunks::<2>().0[4410..39690]
            .iter()
            .map(|f| f[0])
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|p| p[0] <= 0.0 && p[1] > 0.0)
            .count();
        let expected = 440.0 * 2.0f64.powf(f64::from(semitones) / 12.0);
        let measured = crossings as f64 / 0.8;
        assert!(
            (measured - expected).abs() / expected < 0.02,
            "pitch {semitones} preserve={preserve}: expected {expected} Hz, measured {measured} Hz"
        );
    }
    for (format, bits) in [
        ("wav", 16),
        ("wav", 24),
        ("wav", 32),
        ("flac", 16),
        ("flac", 24),
        ("mp3", 16),
    ] {
        for (rate, channels) in [(44100, 1), (44100, 2), (48000, 1), (48000, 2)] {
            for normalize in [false, true] {
                let target = directory.0.join(format!(
                    "export-{rate}-{channels}-{bits}-{normalize}.{format}"
                ));
                tools
                    .export(
                        &source,
                        &target,
                        &ExportOptions::new(format, rate, channels, bits, normalize).unwrap(),
                        &CancellationToken::default(),
                    )
                    .unwrap();
                let stream = encoded_stream(&target);
                assert_eq!(
                    stream["sample_rate"]
                        .as_str()
                        .unwrap()
                        .parse::<u32>()
                        .unwrap(),
                    rate
                );
                assert_eq!(stream["channels"].as_u64().unwrap(), u64::from(channels));
                match format {
                    "wav" => {
                        assert_eq!(stream["bits_per_sample"].as_u64().unwrap(), u64::from(bits))
                    }
                    "flac" => assert_eq!(
                        stream["bits_per_raw_sample"]
                            .as_str()
                            .unwrap()
                            .parse::<u16>()
                            .unwrap(),
                        bits
                    ),
                    "mp3" => assert_eq!(
                        stream["bit_rate"].as_str().unwrap().parse::<u32>().unwrap(),
                        320000
                    ),
                    _ => unreachable!(),
                }
                let decoded = tools
                    .decode_for_separation(&target, &CancellationToken::default())
                    .unwrap();
                assert_eq!(decoded.format(), original.format());
                assert!(decoded.frames().abs_diff(original.frames()) < 2205);
                if format != "mp3" && rate == 44100 && channels == 2 && !normalize {
                    let maximum = decoded
                        .samples()
                        .iter()
                        .zip(original.samples())
                        .map(|(a, b)| (a - b).abs())
                        .fold(0.0, f32::max);
                    let tolerance = if bits == 16 {
                        1.0 / 32768.0
                    } else {
                        1.0 / 8388608.0
                    };
                    assert!(
                        maximum <= tolerance,
                        "{format}/{bits}-bit quantization error: {maximum}"
                    );
                }
            }
        }
    }
    let headroom = AudioBufferF32::new(
        AudioFormat::new(44100, 2).unwrap(),
        vec![1.25, -1.5, 2.0, -2.0],
    )
    .unwrap();
    let float_source = directory.0.join("headroom-source.wav");
    let float_target = directory.0.join("headroom-export.wav");
    write_wav_f32(&float_source, &headroom).unwrap();
    tools
        .export(
            &float_source,
            &float_target,
            &ExportOptions::new("wav", 44100, 2, 32, false).unwrap(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(encoded_stream(&float_target)["codec_name"], "pcm_f32le");
    assert_eq!(
        read_wav_f32(&float_target).unwrap(),
        headroom,
        "float export must preserve values beyond full scale without gain changes"
    );
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

fn encoded_stream(path: &std::path::Path) -> serde_json::Value {
    let ffmpeg = std::path::PathBuf::from(
        std::env::var_os("ATOMUSICATOR_FFMPEG").expect("set the bundled FFmpeg path"),
    );
    let probe = ffmpeg.with_file_name(if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    });
    let mut command = std::process::Command::new(probe);
    command
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_name,sample_rate,channels,bits_per_sample,bits_per_raw_sample,bit_rate",
            "-of",
            "json",
        ])
        .arg(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "FFprobe could not inspect {}",
        path.display()
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    report["streams"][0].clone()
}
