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

pub fn clean_verbatim_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let raw = path.to_string_lossy();
        if let Some(stripped) = raw.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{stripped}"));
        }
        if let Some(stripped) = raw.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }
    path.to_path_buf()
}

pub struct TemporaryDirectory(pub PathBuf);
impl TemporaryDirectory {
    pub fn for_job(parent: &Path, id: &str) -> Result<Self, DomainError> {
        if id.is_empty()
            || id.len() > 160
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
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
        if self.pinned() {
            if let Ok(work) = TemporaryDirectory::new(&std::env::temp_dir()) {
                let output = work.0.join("decoded.wav");
                let clean_path = clean_verbatim_path(path);
                let res = self.run(
                    &[
                        "-i".into(),
                        clean_path.as_os_str().into(),
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
                );
                if res.is_ok() {
                    if let Ok(audio) = read_wav_f32(&output) {
                        if audio.frames() > 0 {
                            return Ok(audio);
                        }
                    }
                } else if cancel.is_cancelled() {
                    return Err(DomainError::SeparationCancelled);
                }
            }
        }
        let decoded = crate::symphonia_decode::decode_buffer(path, cancel)?;
        let audio = crate::symphonia_decode::convert(&decoded, 44100, 2)?;
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
        if self.pinned() {
            let clean_source = clean_verbatim_path(source);
            let ffmpeg_res = self.run_stdout_to_file(
                &[
                    "-i".into(),
                    clean_source.as_os_str().into(),
                    "-map".into(),
                    "0:a:0".into(),
                    "-vn".into(),
                    "-ar".into(),
                    "44100".into(),
                    "-ac".into(),
                    "2".into(),
                    "-f".into(),
                    "f32le".into(),
                    "pipe:1".into(),
                ],
                target,
                cancel,
            );
            if ffmpeg_res.is_ok() {
                if let Ok(spool) = crate::storage::float_spool::FloatSpool::inspect_raw(
                    target.into(),
                    atomusicator_domain::AudioFormat::new(44100, 2)?,
                    cancel,
                ) {
                    return Ok(spool);
                }
            } else if cancel.is_cancelled() {
                return Err(DomainError::SeparationCancelled);
            }
        }
        let _ = fs::remove_file(target);
        crate::symphonia_decode::write_spool(source, target, cancel)?;
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
        let clean_source = clean_verbatim_path(source);
        let clean_target = clean_verbatim_path(target);
        let filter = format!(
            "rubberband=pitch={:.12}:tempo=1.0:formant=shifted",
            2.0f64.powf(f64::from(spec.semitones()) / 12.0),
        );
        self.run(
            &[
                "-i".into(),
                clean_source.as_os_str().into(),
                "-af".into(),
                filter.into(),
                "-c:a".into(),
                "pcm_f32le".into(),
                clean_target.as_os_str().into(),
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
        let clean_source = clean_verbatim_path(source);
        let clean_target = clean_verbatim_path(target);
        let mut args: Vec<std::ffi::OsString> = vec!["-i".into(), clean_source.as_os_str().into()];
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
        args.push(clean_target.as_os_str().into());
        match self.run(&args, target, cancel) {
            Ok(()) => Ok(()),
            Err(err) if !cancel.is_cancelled() && options.format == "wav" && !options.normalize => {
                self.export_without_ffmpeg(source, target, options).or(Err(err))
            }
            Err(err) => Err(err),
        }
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
    fn run_stdout_to_file(
        &self,
        args: &[std::ffi::OsString],
        output: &Path,
        cancel: &CancellationToken,
    ) -> Result<(), DomainError> {
        if cancel.is_cancelled() {
            return Err(DomainError::SeparationCancelled);
        }
        if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
            let _ = fs::create_dir_all(parent);
        }
        let output_file = File::create(output).map_err(io_error)?;
        let log = TemporaryDirectory::new(&std::env::temp_dir())?;
        let error_path = log.0.join("stderr.log");
        let error_file = File::create(&error_path).map_err(io_error)?;
        let clean_args: Vec<std::ffi::OsString> = args
            .iter()
            .map(|arg| clean_verbatim_path(Path::new(arg)).into_os_string())
            .collect();
        let mut command = Command::new(clean_verbatim_path(&self.executable));
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
            .args(&clean_args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(output_file))
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
        self.wait_child(&mut child, &log.0, &error_path, output, cancel)
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
        let clean_args: Vec<std::ffi::OsString> = args
            .iter()
            .map(|arg| clean_verbatim_path(Path::new(arg)).into_os_string())
            .collect();
        let mut command = Command::new(clean_verbatim_path(&self.executable));
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
            .args(&clean_args)
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
        let res = self.wait_child(&mut child, &log.0, &error_path, output, cancel);
        if res.is_err() && !cancel.is_cancelled() && !clean_args.is_empty() {
            // Fallback: if FFmpeg could not open the destination path directly (e.g. Controlled Folder Access),
            // render into the temporary log directory and copy the file via the host process.
            let ext = output
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("tmp");
            let staged_output = log.0.join(format!("staged_output.{ext}"));
            let mut staged_args = clean_args;
            if let Some(last) = staged_args.last_mut() {
                *last = clean_verbatim_path(&staged_output).into_os_string();
            }
            let retry_err_path = log.0.join("stderr_retry.log");
            if let Ok(retry_err_file) = File::create(&retry_err_path) {
                let mut retry_cmd = Command::new(clean_verbatim_path(&self.executable));
                retry_cmd
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
                    .args(&staged_args)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::from(retry_err_file));
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    retry_cmd.creation_flags(0x08000000);
                }
                if let Ok(mut retry_child) = retry_cmd.spawn() {
                    if self
                        .wait_child(&mut retry_child, &log.0, &retry_err_path, &staged_output, cancel)
                        .is_ok()
                        && staged_output.is_file()
                    {
                        if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty())
                        {
                            let _ = fs::create_dir_all(parent);
                        }
                        if fs::copy(&staged_output, output).is_ok() {
                            return Ok(());
                        }
                    }
                }
            }
        }
        res
    }
    fn wait_child(
        &self,
        child: &mut std::process::Child,
        log_dir: &Path,
        error_path: &Path,
        output: &Path,
        cancel: &CancellationToken,
    ) -> Result<(), DomainError> {
        let start = Instant::now();
        loop {
            let output_parent = output
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let remaining = match fs4::available_space(log_dir).and_then(|logs| {
                Ok(match fs4::available_space(output_parent) {
                    Ok(destination) => logs.min(destination),
                    Err(_) => logs,
                })
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
                    if let Ok(file) = File::open(error_path) {
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
    use super::{
        AudioTools, CancellationToken, TemporaryDirectory, clean_verbatim_path,
        directory_of_mapped_library,
    };
    use crate::audio_io::wav_codec::write_wav_f32;
    use atomusicator_domain::{AudioBufferF32, AudioFormat};
    use std::path::Path;

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

    #[test]
    fn temporary_directory_accepts_formal_project_names_with_underscores() {
        let root = TemporaryDirectory::new(&std::env::temp_dir()).expect("temp root");
        let dir = TemporaryDirectory::for_job(
            &root.0,
            "Proyecto_Elefante-Mentirosa_5Stems_20261109-141500",
        )
        .expect("formal project name should be accepted");
        assert!(dir.0.join(".job-owner").is_file());
    }

    #[cfg(windows)]
    #[test]
    fn clean_verbatim_path_strips_extended_length_prefixes() {
        assert_eq!(
            clean_verbatim_path(Path::new(r"\\?\C:\Users\Matius\Desktop\decoded.f32")),
            Path::new(r"C:\Users\Matius\Desktop\decoded.f32")
        );
        assert_eq!(
            clean_verbatim_path(Path::new(r"\\?\UNC\server\share\decoded.f32")),
            Path::new(r"\\server\share\decoded.f32")
        );
    }

    #[test]
    fn decode_to_spool_falls_back_to_symphonia_when_ffmpeg_fails() {
        let root = TemporaryDirectory::new(&std::env::temp_dir()).expect("temp root");
        let source = root.0.join("sample.wav");
        let target = root.0.join("decoded.f32");
        let buffer = AudioBufferF32::new(
            AudioFormat::new(44100, 2).unwrap(),
            vec![0.25f32; 4410 * 2],
        )
        .unwrap();
        write_wav_f32(&source, &buffer).unwrap();

        // Point AudioTools to a non-FFmpeg file (`source` itself) so spawning FFmpeg fails,
        // verifying automatic fallback to native Symphonia decoding.
        let tools = AudioTools::at(source.clone()).unwrap();
        let spool = tools
            .decode_to_spool(&source, &target, &CancellationToken::default())
            .expect("should fall back to symphonia_decode::write_spool");
        use atomusicator_application::ports::AudioBlockSource;
        assert_eq!(spool.metadata().frame_count, 4410);
    }
}
