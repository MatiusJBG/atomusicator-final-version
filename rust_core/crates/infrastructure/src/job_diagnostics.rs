//! Thread-scoped JSONL tracing. No global subscriber, audio callback or PCM logging.
use crate::{errors::InfrastructureError, process_memory::resident_bytes};
use std::{
    path::Path,
    sync::{Mutex, mpsc},
    thread::JoinHandle,
    time::{Duration, Instant},
};
#[derive(Clone, Copy)]
pub struct DiagnosticOptions {
    enabled: bool,
    level: tracing::Level,
    gpu_device: Option<u32>,
}
impl DiagnosticOptions {
    pub fn parse(enabled: Option<&str>, level: Option<&str>) -> Result<Self, InfrastructureError> {
        let enabled = match enabled {
            None | Some("true" | "1") => true,
            Some("false" | "0") => false,
            _ => {
                return Err(InfrastructureError::InvalidManifest(
                    "ATOMUSICATOR_DIAGNOSTICS must be true/false or 1/0".into(),
                ));
            }
        };
        let level = level.unwrap_or("INFO").parse().map_err(|_| {
            InfrastructureError::InvalidManifest(
                "ATOMUSICATOR_LOG_LEVEL must be TRACE/DEBUG/INFO/WARN/ERROR".into(),
            )
        })?;
        Ok(Self {
            enabled,
            level,
            gpu_device: None,
        })
    }
    pub fn with_gpu_device(mut self, device: Option<u32>) -> Self {
        self.gpu_device = device;
        self
    }
    pub fn from_env() -> Result<Self, InfrastructureError> {
        fn value(name: &str) -> Result<Option<String>, InfrastructureError> {
            match std::env::var(name) {
                Ok(v) => Ok(Some(v)),
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(e) => Err(InfrastructureError::InvalidManifest(e.to_string())),
            }
        }
        Self::parse(
            value("ATOMUSICATOR_DIAGNOSTICS")?.as_deref(),
            value("ATOMUSICATOR_LOG_LEVEL")?.as_deref(),
        )
    }
}
pub fn run_job<T>(
    root: &Path,
    id: &str,
    options: DiagnosticOptions,
    operation: impl FnOnce() -> T,
) -> Result<T, InfrastructureError> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(InfrastructureError::InvalidManifest(
            "unsafe diagnostic job ID".into(),
        ));
    }
    if !options.enabled {
        return Ok(operation());
    }
    std::fs::create_dir_all(root).map_err(|e| InfrastructureError::io(root, e))?;
    let path = root.join(format!("{id}.diagnostics.jsonl"));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| InfrastructureError::io(&path, e))?;
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_ansi(false)
        .with_max_level(options.level)
        .with_writer(Mutex::new(file))
        .finish();
    Ok(tracing::subscriber::with_default(subscriber, || {
        let _job = tracing::info_span!("job", job_id = id).entered();
        let _memory = MemorySummary::start(options.gpu_device);
        tracing::info!(
            app_version = env!("CARGO_PKG_VERSION"),
            "job diagnostics started"
        );
        operation()
    }))
}
pub struct StageTimer {
    stage: &'static str,
    start: Option<Instant>,
}
impl StageTimer {
    pub fn new(stage: &'static str) -> Self {
        Self {
            stage,
            start: tracing::enabled!(tracing::Level::INFO).then(Instant::now),
        }
    }
}
impl Drop for StageTimer {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            tracing::info!(
                stage = self.stage,
                seconds = start.elapsed().as_secs_f64(),
                panicking = std::thread::panicking(),
                "stage completed"
            );
        }
    }
}
struct MemorySummary {
    stop: mpsc::Sender<()>,
    worker: Option<JoinHandle<MemorySamples>>,
    start: Instant,
}
struct MemorySamples {
    rss: Option<u64>,
    samples: u64,
    gap_seconds: f64,
    gpu: Option<u64>,
}
impl MemorySummary {
    fn start(gpu_device: Option<u32>) -> Self {
        let (stop, receiver) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("atomusicator-memory-sample".into())
            .spawn(move || {
                let mut peak = resident_bytes();
                let mut gpu_peak = gpu_usage(gpu_device);
                let mut samples = u64::from(peak.is_some());
                let mut gap = 0.0f64;
                let mut last = Instant::now();
                while receiver.recv_timeout(Duration::from_millis(100))
                    == Err(mpsc::RecvTimeoutError::Timeout)
                {
                    let now = Instant::now();
                    gap = gap.max(now.duration_since(last).as_secs_f64());
                    last = now;
                    if let Some(value) = resident_bytes() {
                        peak = Some(peak.unwrap_or(0).max(value));
                        samples += 1;
                    }
                    if let Some(value) = gpu_usage(gpu_device) {
                        gpu_peak = Some(gpu_peak.unwrap_or(0).max(value));
                    }
                }
                MemorySamples {
                    rss: peak,
                    samples,
                    gap_seconds: gap,
                    gpu: gpu_peak,
                }
            })
            .ok();
        Self {
            stop,
            worker,
            start: Instant::now(),
        }
    }
}
impl Drop for MemorySummary {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        let values = self.worker.take().and_then(|worker| worker.join().ok());
        tracing::info!(
            sampled_peak_rss_bytes = values.as_ref().and_then(|v| v.rss),
            rss_observation_available = values.as_ref().and_then(|v| v.rss).is_some(),
            sampled_peak_gpu_process_bytes = values.as_ref().and_then(|v| v.gpu),
            gpu_memory_scope =
                "DXGI local segment current-process usage; sampled, not an allocation-exact peak",
            sample_count = values.as_ref().map(|v| v.samples).unwrap_or(0),
            max_sample_gap_seconds = values.as_ref().map(|v| v.gap_seconds),
            elapsed_seconds = self.start.elapsed().as_secs_f64(),
            memory_scope = "current process only; excludes child processes and VRAM",
            panicking = std::thread::panicking(),
            "job diagnostic summary"
        );
    }
}
fn gpu_usage(device: Option<u32>) -> Option<u64> {
    crate::gpu_probe::adapter(device?)
        .ok()??
        .current_process_usage_bytes
}
