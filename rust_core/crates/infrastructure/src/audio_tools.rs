//! FFmpeg audio conversion and tempo-preserving pitch. No shell or Python runtime.
use crate::audio_io::wav_codec::{read_wav_f32, wav_metadata};
use atomusicator_application::CancellationToken;
use atomusicator_domain::{AudioBufferF32, DomainError, PitchShiftSpec};
use std::{
    fs::{self, File},
    io::Read,
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
    pub fn for_job(parent: &Path, id: &str) -> Result<Self, DomainError> {
        if id.is_empty()
            || id.len() > 160
            || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(io_error("invalid temporary job owner"));
        }
        let directory = Self::new(parent)?;
        let mut marker = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(directory.0.join(".job-owner"))
            .map_err(io_error)?;
        use std::io::Write;
        marker.write_all(id.as_bytes()).map_err(io_error)?;
        marker.sync_all().map_err(io_error)?;
        Ok(directory)
    }
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
            || !(matches!(bit_depth, 16 | 24) || format == "wav" && bit_depth == 32)
        {
            return Err(io_error(
                "export requires WAV/FLAC/MP3, 44100/48000 Hz, mono/stereo and 16/24 bits (32-bit float is supported for WAV)",
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
            let path = PathBuf::from(path);
            if !path.is_file() {
                return Err(io_error(format!(
                    "ATOMUSICATOR_FFMPEG is not a file: {}",
                    path.display()
                )));
            }
            return Self::at(path);
        }
        if let Some(path) = pinned_ffmpeg_beside_host() {
            return Self::at(path);
        }
        Ok(Self {
            executable: PathBuf::new(),
        })
    }
    /// Decoder and WAV writer used when no pinned FFmpeg binary is installed.
    pub fn without_pinned_binary() -> Self {
        Self {
            executable: PathBuf::new(),
        }
    }
    pub fn at(executable: PathBuf) -> Result<Self, DomainError> {
        Ok(Self { executable })
    }
    fn pinned(&self) -> bool {
        self.executable.is_file()
    }
    pub fn pitch_runtime_signature(
        &self,
        cancel: &CancellationToken,
    ) -> Result<String, DomainError> {
        let executable = if self.executable.is_file() {
            self.executable.clone()
        } else if self.executable.as_os_str().is_empty() {
            return Err(io_error("FFmpeg executable cannot be fingerprinted"));
        } else {
            let paths = std::env::var_os("PATH").ok_or_else(|| io_error("PATH is unavailable"))?;
            std::env::split_paths(&paths)
                .flat_map(|directory| {
                    let candidate = directory.join(&self.executable);
                    [candidate.clone(), candidate.with_extension("exe")]
                })
                .find(|path| path.is_file())
                .ok_or_else(|| io_error("FFmpeg executable cannot be fingerprinted"))?
        };
        Ok(format!(
            "rubberband-tempo1-pitch-v1-ffmpeg-sha256-{}",
            crate::storage::checksum::file_hash(&executable, cancel)?
        ))
    }
    pub fn decode_for_separation(
        &self,
        path: &Path,
        cancel: &CancellationToken,
    ) -> Result<AudioBufferF32, DomainError> {
        if !self.pinned() {
            let decoded = crate::symphonia_decode::decode_buffer(path, cancel)?;
            let audio = crate::symphonia_decode::convert(&decoded, 44100, 2)?;
            if audio.frames() == 0 {
                return Err(DomainError::AudioDecodeFailure("audio is empty".into()));
            }
            return Ok(audio);
        }
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
            &output,
            cancel,
        )?;
        let audio = read_wav_f32(&output)?;
        if audio.frames() == 0 {
            return Err(DomainError::AudioDecodeFailure("audio is empty".into()));
        }
        Ok(audio)
    }
    pub fn decode_to_spool(
        &self,
        source: &Path,
        target: &Path,
        cancel: &CancellationToken,
    ) -> Result<crate::storage::float_spool::FloatSpool, DomainError> {
        let _timer = crate::job_diagnostics::StageTimer::new("decode_audio");
        if !self.pinned() {
            crate::symphonia_decode::write_spool(source, target, cancel)?;
            return crate::storage::float_spool::FloatSpool::inspect_raw(
                target.into(),
                atomusicator_domain::AudioFormat::new(44100, 2)?,
                cancel,
            );
        }
        self.run(
            &[
                "-i".into(),
                source.as_os_str().into(),
                "-map".into(),
                "0:a:0".into(),
                "-vn".into(),
                "-ar".into(),
                "44100".into(),
                "-ac".into(),
                "2".into(),
                "-f".into(),
                "f32le".into(),
                target.as_os_str().into(),
            ],
            target,
            cancel,
        )?;
        crate::storage::float_spool::FloatSpool::inspect_raw(
            target.into(),
            atomusicator_domain::AudioFormat::new(44100, 2)?,
            cancel,
        )
    }
    pub fn pitch(
        &self,
        source: &Path,
        target: &Path,
        spec: PitchShiftSpec,
        cancel: &CancellationToken,
    ) -> Result<(), DomainError> {
        if !self.pinned() {
            return Err(io_error(
                "tempo-preserving pitch requires the pinned FFmpeg binary",
            ));
        }
        let filter = format!(
            "rubberband=pitch={:.12}:tempo=1.0:formant=shifted",
            2.0f64.powf(f64::from(spec.semitones()) / 12.0),
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
            target,
            cancel,
        )?;
        let original = wav_metadata(source)?;
        let rendered = wav_metadata(target)?;
        if original.format != rendered.format
            || original.frame_count.abs_diff(rendered.frame_count) > 2
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
        if !self.pinned() {
            return self.export_without_ffmpeg(source, target, options);
        }
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
            "wav" => args.push(if options.bit_depth == 32 {
                "pcm_f32le".into()
            } else {
                format!("pcm_s{}le", options.bit_depth).into()
            }),
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
        if options.format == "wav" {
            args.extend(["-rf64".into(), "auto".into()]);
        }
        args.push(target.as_os_str().into());
        self.run(&args, target, cancel)
    }
    fn export_without_ffmpeg(
        &self,
        source: &Path,
        target: &Path,
        options: &ExportOptions,
    ) -> Result<(), DomainError> {
        if options.normalize {
            return Err(io_error(
                "loudnorm requires the pinned FFmpeg binary",
            ));
        }
        match options.format.as_str() {
            "wav" => {}
            "flac" => {
                return Err(io_error(
                    "FLAC export requires the pinned FFmpeg binary",
                ));
            }
            "mp3" => {
                return Err(io_error(
                    "MP3 export requires the pinned FFmpeg binary",
                ));
            }
            _ => return Err(io_error("unsupported export format")),
        }
        let decoded = read_wav_f32(source)?;
        let audio =
            crate::symphonia_decode::convert(&decoded, options.sample_rate, options.channels)?;
        crate::audio_io::wav_codec::write_export_wav(target, &audio, options.bit_depth)
    }
    fn run(
        &self,
        args: &[std::ffi::OsString],
        output: &Path,
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
            let output_parent = output
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let remaining = match fs4::available_space(&log.0).and_then(|logs| {
                fs4::available_space(output_parent).map(|destination| logs.min(destination))
            }) {
                Ok(available) => available,
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(io_error(error));
                }
            };
            if remaining < 32 * 1024 * 1024 {
                let _ = child.kill();
                let _ = child.wait();
                return Err(DomainError::InsufficientResources {
                    resource: "temporary disk safety margin".into(),
                    requested: 32 * 1024 * 1024,
                    available: remaining,
                });
            }
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
                    let mut message = String::new();
                    if let Ok(file) = File::open(&error_path) {
                        let _ = file.take(16384).read_to_string(&mut message);
                    }
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

fn ffmpeg_file_name() -> &'static str {
    if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }
}

/// Directory containing `libatomusicator_bootstrap_ffi.so`, read from a `/proc/self/maps` snapshot.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn directory_of_mapped_library(maps: &str, file_name: &str) -> Option<PathBuf> {
    for line in maps.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 {
            continue;
        }
        let path = parts[5..].join(" ");
        if path.ends_with(file_name) {
            return Path::new(&path).parent().map(Path::to_path_buf);
        }
    }
    None
}

fn pinned_ffmpeg_beside_host() -> Option<PathBuf> {
    let mut directories = Vec::new();
    #[cfg(target_os = "android")]
    if let Ok(maps) = fs::read_to_string("/proc/self/maps")
        && let Some(directory) =
            directory_of_mapped_library(&maps, "libatomusicator_bootstrap_ffi.so")
    {
        directories.push(directory);
    }
    #[cfg(not(target_os = "android"))]
    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
    {
        directories.push(parent.to_path_buf());
    }
    let name = ffmpeg_file_name();
    for parent in &directories {
        for path in [
            parent.join("audio-tools").join("bin").join(name),
            parent.join(name),
            parent.join("libffmpeg.so"),
        ] {
            if path.is_file() {
                return Some(path);
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    if let Some(path) = ffmpeg_on_path(name) {
        return Some(path);
    }
    None
}

#[cfg(not(target_os = "android"))]
fn ffmpeg_on_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).find_map(|directory| {
        let candidate = directory.join(name);
        if candidate.is_file() {
            Some(candidate)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod maps_tests {
    use super::directory_of_mapped_library;

    #[test]
    fn maps_line_locates_the_android_bridge_beside_its_directory() {
        let maps = "\
00400000-00401000 r-xp 00000000 fd:00 1 /system/bin/app_process64\n\
7b1000-7b2000 r-xp 00000000 fd:00 9 /data/app/~~x/lib/arm64/libatomusicator_bootstrap_ffi.so\n";
        let directory = directory_of_mapped_library(maps, "libatomusicator_bootstrap_ffi.so")
            .expect("library path");
        assert!(
            directory.ends_with("arm64"),
            "{}",
            directory.display()
        );
    }
}
