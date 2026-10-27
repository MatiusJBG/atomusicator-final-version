//! FFmpeg audio conversion and tempo-preserving pitch. No shell or Python runtime.
use crate::audio_io::wav_codec::read_wav_f32;
use atomusicator_application::CancellationToken;
use atomusicator_domain::{AudioBufferF32, DomainError, PitchShiftSpec};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);
pub fn unique_id() -> String {
    format!(
        "{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    )
}

pub struct TemporaryDirectory(pub PathBuf);
impl TemporaryDirectory {
    pub fn new(parent: &Path) -> Result<Self, DomainError> {
        let path = parent.join(format!(".atomusicator-{}", unique_id()));
        fs::create_dir_all(&path).map_err(io_error)?;
        Ok(Self(path))
    }
}
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub format: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub bit_depth: u16,
    pub normalize: bool,
}
impl ExportOptions {
    pub fn new(
        format: &str,
        sample_rate: u32,
        channels: u16,
        bit_depth: u16,
        normalize: bool,
    ) -> Result<Self, DomainError> {
        if !matches!(format, "wav" | "flac" | "mp3")
            || !matches!(sample_rate, 44100 | 48000)
            || !matches!(channels, 1 | 2)
            || !matches!(bit_depth, 16 | 24)
        {
            return Err(io_error(
                "export requires WAV/FLAC/MP3, 44100/48000 Hz, mono/stereo and 16/24 bits",
            ));
        }
        Ok(Self {
            format: format.into(),
            sample_rate,
            channels,
            bit_depth,
            normalize,
        })
    }
}
pub struct AudioTools {
    executable: PathBuf,
}
impl AudioTools {
    pub fn discover() -> Result<Self, DomainError> {
        if let Some(path) = std::env::var_os("ATOMUSICATOR_FFMPEG") {
            return Self::at(PathBuf::from(path));
        }
        if let Ok(executable) = std::env::current_exe() {
            if let Some(parent) = executable.parent() {
                let name = if cfg!(windows) {
                    "ffmpeg.exe"
                } else {
                    "ffmpeg"
                };
                for path in [parent.join("audio-tools/bin").join(name), parent.join(name)] {
                    if path.is_file() {
                        return Self::at(path);
                    }
                }
            }
        }
        // Development fallback; packaged builds always include their pinned tools.
        Self::at(PathBuf::from("ffmpeg"))
    }
    pub fn at(executable: PathBuf) -> Result<Self, DomainError> {
        Ok(Self { executable })
    }
    pub fn decode_for_separation(
        &self,
        path: &Path,
        cancel: &CancellationToken,
    ) -> Result<AudioBufferF32, DomainError> {
        let work = TemporaryDirectory::new(&std::env::temp_dir())?;
        let output = work.0.join("decoded.wav");
        self.run(
            &[
                "-i".into(),
                path.as_os_str().into(),
                "-map".into(),
                "0:a:0".into(),
                "-vn".into(),
                "-ar".into(),
                "44100".into(),
                "-ac".into(),
                "2".into(),
                "-c:a".into(),
                "pcm_f32le".into(),
                output.as_os_str().into(),
            ],
            cancel,
        )?;
        let audio = read_wav_f32(&output)?;
        if audio.frames() == 0 {
            return Err(DomainError::AudioDecodeFailure("audio is empty".into()));
        }
        Ok(audio)
    }
    pub fn pitch(
        &self,
        source: &Path,
        target: &Path,
        spec: PitchShiftSpec,
        cancel: &CancellationToken,
    ) -> Result<(), DomainError> {
        let filter = format!(
            "rubberband=pitch={:.12}:tempo=1.0:formant={}",
            2.0f64.powf(f64::from(spec.semitones()) / 12.0),
            if spec.preserve_formants() {
                "preserved"
            } else {
                "shifted"
            }
        );
        self.run(
            &[
                "-i".into(),
                source.as_os_str().into(),
                "-af".into(),
                filter.into(),
                "-c:a".into(),
                "pcm_f32le".into(),
                target.as_os_str().into(),
            ],
            cancel,
        )?;
        let original = read_wav_f32(source)?;
        let rendered = read_wav_f32(target)?;
        if original.format() != rendered.format()
            || original.frames().abs_diff(rendered.frames()) > 2
        {
            return Err(io_error(
                "pitch render changed the audio duration or format",
            ));
        }
        Ok(())
    }
    pub fn export(
        &self,
        source: &Path,
        target: &Path,
        options: &ExportOptions,
        cancel: &CancellationToken,
    ) -> Result<(), DomainError> {
        let mut args: Vec<std::ffi::OsString> = vec!["-i".into(), source.as_os_str().into()];
        if options.normalize {
            args.extend(["-af".into(), "loudnorm=I=-16:TP=-1:LRA=11".into()]);
        }
        args.extend([
            "-ar".into(),
            options.sample_rate.to_string().into(),
            "-ac".into(),
            options.channels.to_string().into(),
            "-c:a".into(),
        ]);
        match options.format.as_str() {
            "wav" => args.push(format!("pcm_s{}le", options.bit_depth).into()),
            "flac" => args.extend([
                "flac".into(),
                "-sample_fmt".into(),
                if options.bit_depth == 16 {
                    "s16".into()
                } else {
                    "s32".into()
                },
                "-bits_per_raw_sample".into(),
                options.bit_depth.to_string().into(),
            ]),
            "mp3" => args.extend(["libmp3lame".into(), "-b:a".into(), "320k".into()]),
            _ => return Err(io_error("unsupported export format")),
        }
        args.push(target.as_os_str().into());
        self.run(&args, cancel)
    }
    fn run(
        &self,
        args: &[std::ffi::OsString],
        cancel: &CancellationToken,
    ) -> Result<(), DomainError> {
        if cancel.is_cancelled() {
            return Err(DomainError::SeparationCancelled);
        }
        let log = TemporaryDirectory::new(&std::env::temp_dir())?;
        let error_path = log.0.join("stderr.log");
        let error_file = File::create(&error_path).map_err(io_error)?;
        let mut command = Command::new(&self.executable);
        command
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-threads",
                "2",
                "-filter_threads",
                "1",
            ])
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(error_file));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|e| {
            io_error(format!(
                "FFmpeg unavailable ({}): {e}",
                self.executable.display()
            ))
        })?;
        let start = Instant::now();
        loop {
            if cancel.is_cancelled() || start.elapsed() > Duration::from_secs(3600) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(if cancel.is_cancelled() {
                    DomainError::SeparationCancelled
                } else {
                    io_error("FFmpeg timed out")
                });
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    if status.success() {
                        return Ok(());
                    }
                    let message = fs::read_to_string(&error_path).unwrap_or_default();
                    return Err(io_error(format!(
                        "FFmpeg failed: {}",
                        message.chars().take(4000).collect::<String>()
                    )));
                }
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(io_error(error));
                }
            }
        }
    }
}
fn io_error(error: impl std::fmt::Display) -> DomainError {
    DomainError::AudioExportFailure(error.to_string())
}
