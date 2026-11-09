use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

use crate::session_pool::SessionPool;
use ort::{
    execution_providers::{CPUExecutionProvider, XNNPACKExecutionProvider},
    memory::{AllocationDevice, AllocatorType, MemoryInfo, MemoryType},
    session::{Session, builder::{GraphOptimizationLevel, SessionBuilder}},
    value::TensorRefMut,
};
#[cfg(target_os = "windows")]
use ort::execution_providers::{CUDAExecutionProvider, DirectMLExecutionProvider};
#[cfg(any(target_os = "macos", target_os = "ios"))]
use ort::execution_providers::CoreMLExecutionProvider;

use crate::errors::InfrastructureError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ExecutionProviderKind {
    DirectMl,
    Cuda,
    CoreMl,
    Xnnpack,
    Cpu,
}

pub struct OrtSessionPool {
    sessions: SessionPool<Mutex<PooledSession>, SessionKey>,
    provider_order: Vec<ExecutionProviderKind>,
    intra_threads: AtomicUsize,
    profiling_directory: Option<PathBuf>,
    playback_probe: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    diagnostics: Mutex<BTreeMap<String, SessionDiagnostic>>,
    quarantined: Mutex<BTreeMap<PathBuf, String>>,
    device_id: u32,
    require_accelerator: bool,
    compute_limits: Mutex<atomusicator_application::compute_policy::ComputeLimits>,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct SessionKey {
    path: PathBuf,
    threads: usize,
    force_cpu: bool,
}
pub struct PooledSession {
    session: Session,
    provider: ExecutionProviderKind,
    diagnostic_key: String,
}
impl std::ops::Deref for PooledSession {
    type Target = Session;
    fn deref(&self) -> &Session {
        &self.session
    }
}
impl std::ops::DerefMut for PooledSession {
    fn deref_mut(&mut self) -> &mut Session {
        &mut self.session
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionDiagnostic {
    pub gpu_memory: Option<crate::gpu_probe::AdapterMemory>,
    pub sampled_peak_gpu_process_bytes: Option<u64>,
    pub device_id: Option<u32>,
    pub intra_threads: usize,
    pub provider_registered: ExecutionProviderKind,
    /// CPU-only sessions have an unambiguous assignment. Accelerator sessions
    /// may partition nodes onto CPU; registration alone cannot prove assignment.
    pub provider_effective: Option<String>,
    pub fallback_errors: Vec<String>,
    pub initialization_seconds: f64,
}

/// Scope this guard to an offline job. Drop after model runners and leases so
/// idle ORT workspaces are released even on cancellation or unwinding.
pub struct IdleSessionScope {
    pool: Arc<OrtSessionPool>,
}
impl Drop for IdleSessionScope {
    fn drop(&mut self) {
        let result = self
            .pool
            .capacity()
            .and_then(|capacity| self.pool.trim_idle_and_resize(capacity));
        if let Err(error) = result {
            tracing::warn!(%error,"could not release idle ONNX sessions");
        }
    }
}

impl OrtSessionPool {
    pub fn gpu_device_id(&self) -> Option<u32> {
        self.provider_order
            .contains(&ExecutionProviderKind::DirectMl)
            .then_some(self.device_id)
    }
    pub fn using_execution_options(
        self,
        options: crate::preferences::ExecutionOptions,
    ) -> Result<Self, InfrastructureError> {
        use crate::preferences::ExecutionMode;
        if cfg!(target_os = "android") && options.mode != ExecutionMode::Cpu {
            return Err(InfrastructureError::InvalidManifest(
                "Android inference is CPU only".into(),
            ));
        }
        if options.mode == ExecutionMode::Cpu {
            #[cfg(target_os = "android")]
            return self.using_provider_order(vec![ExecutionProviderKind::Cpu]);
            #[cfg(not(target_os = "android"))]
            return self.using_provider_order(vec![ExecutionProviderKind::Cpu]);
        }
        // Qualification and admission currently cover one resident model. Do
        // not retain an unused accelerated model beside its successor.
        self.sessions.trim(1)?;
        let mut device = options.device_id;
        if options.mode == ExecutionMode::Auto {
            let mut candidates = Vec::new();
            for index in 0..16 {
                match crate::gpu_probe::adapter(index) {
                    Ok(Some(adapter)) if !adapter.software => candidates.push(adapter),
                    Ok(Some(_)) => {}
                    _ => break,
                }
            }
            if let Some(best) = candidates
                .into_iter()
                .max_by_key(|value| value.dedicated_bytes)
            {
                device = best.index;
            }
        }
        self.with_device_id(device)?.using_provider_order(vec![
            ExecutionProviderKind::DirectMl,
            ExecutionProviderKind::Cpu,
        ])
    }
    /// Job admission serializes this reset against every offline inference.
    pub fn begin_job_diagnostics(&self) -> Result<(), InfrastructureError> {
        self.diagnostics
            .lock()
            .map_err(|_| InfrastructureError::OnnxRuntime("diagnostic lock poisoned".into()))?
            .clear();
        self.quarantined
            .lock()
            .map_err(|_| {
                InfrastructureError::OnnxRuntime("provider quarantine lock poisoned".into())
            })?
            .clear();
        Ok(())
    }
    pub fn idle_scope(self: &Arc<Self>) -> IdleSessionScope {
        IdleSessionScope { pool: self.clone() }
    }
    pub fn new(capacity: usize) -> Result<Self, InfrastructureError> {
        // Detection is not qualification. Accelerators require explicit opt-in.
        #[cfg(target_os = "android")]
        // MDX-Net's 4D MatMul (TDF) nodes are rejected by XNNPACK and Level3 fuses
        // Conv+BN+Relu into CPU-only NhwcFusedConv, so a mixed XNNPACK/CPU graph only
        // adds NCHW<->NHWC transposes. A single multi-threaded CPU EP avoids them.
        let provider_order = vec![ExecutionProviderKind::Cpu];
        #[cfg(not(target_os = "android"))]
        let provider_order = vec![ExecutionProviderKind::Cpu];
        let logical_cpus = logical_cpu_count();
        let intra_threads =
            atomusicator_application::compute_policy::ComputeBudget::interactive(logical_cpus)
                .threads_per_inference();
        Ok(Self {
            sessions: SessionPool::new(capacity)?,
            provider_order,
            intra_threads: AtomicUsize::new(intra_threads),
            profiling_directory: None,
            playback_probe: None,
            diagnostics: Mutex::new(BTreeMap::new()),
            quarantined: Mutex::new(BTreeMap::new()),
            device_id: 0,
            require_accelerator: false,
            compute_limits: Mutex::new(atomusicator_application::compute_policy::ComputeLimits::desktop()),
        })
    }
    /// Qualification runs must not silently measure CPU after GPU failure.
    pub fn requiring_accelerator(mut self) -> Self {
        self.require_accelerator = true;
        self
    }

    /// Opt-in diagnostic traces; normal application runs do not generate them.
    pub fn with_profiling_directory(
        mut self,
        directory: PathBuf,
    ) -> Result<Self, InfrastructureError> {
        std::fs::create_dir_all(&directory).map_err(|e| InfrastructureError::io(&directory, e))?;
        self.profiling_directory = Some(directory);
        Ok(self)
    }

    /// The callback only reads transport state. Never call into the mixer or
    /// acquire its control mutex from an ORT worker.
    pub fn with_playback_probe(mut self, probe: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        self.playback_probe = Some(probe);
        self
    }

    pub fn compute_limits(&self) -> atomusicator_application::compute_policy::ComputeLimits {
        self.compute_limits
            .lock()
            .map(|l| *l)
            .unwrap_or_else(|e| *e.into_inner())
    }

    pub fn using_compute_limits(
        self,
        limits: atomusicator_application::compute_policy::ComputeLimits,
    ) -> Self {
        if let Ok(mut guard) = self.compute_limits.lock() {
            *guard = limits;
        }
        self
    }

    pub fn set_compute_limits(
        &self,
        limits: atomusicator_application::compute_policy::ComputeLimits,
    ) {
        if let Ok(mut guard) = self.compute_limits.lock() {
            *guard = limits;
        }
    }

    pub fn set_compute_profile_with_limits(
        &self,
        profile: atomusicator_application::compute_policy::ComputeProfile,
        limits: atomusicator_application::compute_policy::ComputeLimits,
    ) {
        let logical = logical_cpu_count();
        let threads = atomusicator_application::compute_policy::ComputeBudget::for_profile_with_limits(
            profile, logical, false, limits,
        )
        .threads_per_inference();
        if let Ok(mut guard) = self.compute_limits.lock() {
            *guard = limits;
        }
        self.intra_threads.store(threads, Ordering::Release);
    }

    pub fn effective_intra_threads(&self) -> usize {
        let requested = self.intra_threads();
        if self.playback_probe.as_ref().is_some_and(|probe| probe()) {
            let logical = logical_cpu_count();
            atomusicator_application::compute_policy::ComputeBudget::playback_limit(
                requested, logical,
            )
        } else {
            requested
        }
    }
    /// Callers must reserve the largest enabled profile and serialize settings
    /// changes against job admission. Existing sessions retain their own key.
    pub fn set_compute_profile(
        &self,
        profile: atomusicator_application::compute_policy::ComputeProfile,
    ) {
        let limits = self.compute_limits();
        self.set_compute_profile_with_limits(profile, limits);
    }

    pub fn with_compute_profile(
        capacity: usize,
        profile: atomusicator_application::compute_policy::ComputeProfile,
    ) -> Result<Self, InfrastructureError> {
        Self::with_compute_profile_and_limits(
            capacity,
            profile,
            atomusicator_application::compute_policy::ComputeLimits::desktop(),
        )
    }

    pub fn with_compute_profile_and_limits(
        capacity: usize,
        profile: atomusicator_application::compute_policy::ComputeProfile,
        limits: atomusicator_application::compute_policy::ComputeLimits,
    ) -> Result<Self, InfrastructureError> {
        let logical = logical_cpu_count();
        let threads = atomusicator_application::compute_policy::ComputeBudget::for_profile_with_limits(
            profile, logical, false, limits,
        )
        .threads_per_inference();
        let pool = Self::with_cpu_threads(capacity, threads)?;
        if let Ok(mut guard) = pool.compute_limits.lock() {
            *guard = limits;
        }
        Ok(pool)
    }

    pub fn with_compute_budget(
        capacity: usize,
        budget: atomusicator_application::compute_policy::ComputeBudget,
    ) -> Result<Self, InfrastructureError> {
        Self::with_cpu_threads(capacity, budget.threads_per_inference())
    }

    pub fn with_provider_order(
        capacity: usize,
        provider_order: Vec<ExecutionProviderKind>,
    ) -> Result<Self, InfrastructureError> {
        Self::new(capacity)?.using_provider_order(provider_order)
    }
    pub fn using_provider_order(
        mut self,
        provider_order: Vec<ExecutionProviderKind>,
    ) -> Result<Self, InfrastructureError> {
        if provider_order.last() != Some(&ExecutionProviderKind::Cpu) {
            return Err(InfrastructureError::InvalidManifest(
                "execution provider order must end with the CPU fallback".into(),
            ));
        }
        self.provider_order = provider_order;
        Ok(self)
    }

    /// Explicit CPU configuration for controlled measurements; production keeps its default policy.
    pub fn with_cpu_threads(
        capacity: usize,
        intra_threads: usize,
    ) -> Result<Self, InfrastructureError> {
        let logical = logical_cpu_count();
        if intra_threads == 0 || intra_threads > logical.min(12) {
            return Err(InfrastructureError::InvalidManifest(
                "CPU intra threads must be between one and min(logical CPUs, twelve)".into(),
            ));
        }
        let mut pool = Self::new(capacity)?;
        pool.intra_threads = AtomicUsize::new(intra_threads);
        Ok(pool)
    }

    pub fn load_session(
        &self,
        model_path: &Path,
    ) -> Result<Arc<Mutex<PooledSession>>, InfrastructureError> {
        self.load_session_mode(model_path, false)
    }
    fn load_session_mode(
        &self,
        model_path: &Path,
        force_cpu: bool,
    ) -> Result<Arc<Mutex<PooledSession>>, InfrastructureError> {
        let canonical_path = model_path
            .canonicalize()
            .map_err(|error| InfrastructureError::io(model_path, error))?;
        let threads = self.effective_intra_threads();
        let quarantined = self
            .quarantined
            .lock()
            .map_err(|_| {
                InfrastructureError::OnnxRuntime("provider quarantine lock poisoned".into())
            })?
            .get(&canonical_path)
            .cloned();
        let force_cpu = force_cpu || quarantined.is_some();
        self.sessions.load(SessionKey { path: canonical_path.clone(), threads, force_cpu }, || {
            let started = Instant::now();
            let mut failures: Vec<String> = quarantined.into_iter().collect();
            let providers = if force_cpu { &[ExecutionProviderKind::Cpu][..] } else { &self.provider_order };
            for provider in providers.iter().copied() {
            if self.require_accelerator && provider==ExecutionProviderKind::Cpu {
                return Err(format!("qualification requires an accelerator; CPU fallback refused: {}",failures.join("; ")));
            }
            if provider == ExecutionProviderKind::DirectMl && let Some(reason) = self.directml_admission_reason() {
                failures.push(format!("DirectMl admission: {reason}")); continue;
            }
            promote_android_compute_thread();
            let ort_intra_threads = threads;
            let result = SessionBuilder::new()
                .and_then(|builder| match &self.profiling_directory {
                    Some(directory) => builder.with_profiling(directory.join(crate::audio_tools::unique_id())),
                    None => Ok(builder),
                })
                .and_then(|builder| builder.with_optimization_level(GraphOptimizationLevel::Level3))
                .and_then(|builder| builder.with_intra_threads(ort_intra_threads))
                .and_then(|builder| builder.with_inter_threads(1))
                .and_then(|builder| builder.with_parallel_execution(false))
                .and_then(|builder| builder.with_intra_op_spinning(cfg!(target_os = "android")))
                .and_then(|builder| builder.with_inter_op_spinning(false))
                .and_then(|builder| {
                    builder.with_memory_pattern(provider != ExecutionProviderKind::DirectMl)
                })
                .and_then(|builder| builder.with_execution_providers([provider_dispatch(provider, self.device_id, threads).error_on_failure()]))
                .and_then(|builder| builder.commit_from_file(&canonical_path));
            match result {
                Ok(session) => {
                    let diagnostic = SessionDiagnostic {
                        gpu_memory: if provider == ExecutionProviderKind::DirectMl { crate::gpu_probe::adapter(self.device_id).ok().flatten() } else { None },
                        sampled_peak_gpu_process_bytes: None,
                        device_id: (provider == ExecutionProviderKind::DirectMl).then_some(self.device_id),
                        intra_threads: threads,
                        provider_registered: provider,
                        provider_effective: match provider {
                            ExecutionProviderKind::Cpu => Some("CPUExecutionProvider".into()),
                            ExecutionProviderKind::Xnnpack => Some("XNNPACKExecutionProvider".into()),
                            _ => None,
                        },
                        fallback_errors: failures,
                        initialization_seconds: started.elapsed().as_secs_f64(),
                    };
                    tracing::info!(provider = ?provider, seconds = diagnostic.initialization_seconds, fallback = ?diagnostic.fallback_errors, "ONNX session initialized");
                    if let Ok(mut diagnostics) = self.diagnostics.lock() {
                        // Bound telemetry independently of the model paths seen.
                        if diagnostics.len() >= 64 { diagnostics.clear(); }
                        let key=format!("{}#{threads}#{force_cpu}", canonical_path.display());
                        let mut diagnostic=diagnostic;
                        diagnostic.sampled_peak_gpu_process_bytes=diagnostics.get(&key).and_then(|v|v.sampled_peak_gpu_process_bytes);
                        diagnostics.insert(key, diagnostic);
                    }
                    return Ok(Mutex::new(PooledSession { session, provider, diagnostic_key:format!("{}#{threads}#{force_cpu}",canonical_path.display()) }));
                }
                Err(error) => failures.push(format!("{provider:?}: {error}")),
            }
            }
            Err(failures.join("; "))
        })
    }
    pub fn with_device_id(mut self, index: u32) -> Result<Self, InfrastructureError> {
        if index > i32::MAX as u32 {
            return Err(InfrastructureError::InvalidManifest(
                "GPU adapter index exceeds i32".into(),
            ));
        }
        self.device_id = index;
        Ok(self)
    }
    fn directml_admission_reason(&self) -> Option<String> {
        use atomusicator_application::gpu_admission::{GpuBudget, admission_reason};
        let adapter = match crate::gpu_probe::adapter(self.device_id) {
            Ok(value) => value,
            Err(error) => return Some(error.to_string()),
        };
        let budget = adapter.and_then(|v| {
            Some(GpuBudget {
                dedicated_bytes: v.dedicated_bytes,
                software: v.software,
                budget_bytes: v.budget_bytes?,
                used_bytes: v.current_process_usage_bytes?,
            })
        });
        let ram = match crate::resource_probe::available_memory_bytes() {
            Ok(value) => value,
            Err(error) => return Some(error.to_string()),
        };
        admission_reason(budget, ram).map(str::to_owned)
    }
    fn quarantine(
        &self,
        path: &Path,
        error: &InfrastructureError,
    ) -> Result<(), InfrastructureError> {
        if self.require_accelerator {
            return Err(InfrastructureError::OnnxRuntime(
                "qualification requires an accelerator; CPU retry refused".into(),
            ));
        }
        let path = path
            .canonicalize()
            .map_err(|e| InfrastructureError::io(path, e))?;
        let mut map = self.quarantined.lock().map_err(|_| {
            InfrastructureError::OnnxRuntime("provider quarantine lock poisoned".into())
        })?;
        if map.len() >= 64 {
            map.clear();
        }
        map.insert(path, format!("accelerator Run failed; CPU retry: {error}"));
        drop(map);
        self.sessions.trim(self.capacity()?)?;
        crate::resource_probe::ensure_memory(1536 * 1024 * 1024, 512 * 1024 * 1024)
            .map_err(|e| InfrastructureError::OnnxRuntime(e.to_string()))?;
        Ok(())
    }
    fn observe_gpu_memory(&self, key: &str) {
        if let Ok(Some(info)) = crate::gpu_probe::adapter(self.device_id)
            && let Ok(mut diagnostics) = self.diagnostics.lock()
            && let Some(diagnostic) = diagnostics.get_mut(key)
        {
            if let Some(bytes) = info.current_process_usage_bytes {
                diagnostic.sampled_peak_gpu_process_bytes = Some(
                    diagnostic
                        .sampled_peak_gpu_process_bytes
                        .unwrap_or(0)
                        .max(bytes),
                );
            }
            diagnostic.gpu_memory = Some(info);
        }
    }

    pub fn diagnostics(&self) -> Vec<SessionDiagnostic> {
        self.diagnostics
            .lock()
            .map(|values| values.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn provider_order(&self) -> &[ExecutionProviderKind] {
        &self.provider_order
    }

    /// Release idle ORT allocations under pressure. Live leases remain counted
    /// and cannot be evicted to pretend that another heavyweight model fits.
    pub fn trim_idle_and_resize(&self, capacity: usize) -> Result<(), InfrastructureError> {
        self.sessions.trim(capacity)
    }
    pub fn capacity(&self) -> Result<usize, InfrastructureError> {
        self.sessions.capacity()
    }
    pub fn intra_threads(&self) -> usize {
        self.intra_threads.load(Ordering::Acquire)
    }
}

pub fn promote_android_compute_thread() {
    #[cfg(target_os = "android")]
    {
        static PERF_CORE_MASK: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        let mask = *PERF_CORE_MASK.get_or_init(|| {
            let mut freqs = Vec::new();
            for cpu in 0..16usize {
                let path = format!("/sys/devices/system/cpu/cpu{cpu}/cpufreq/cpuinfo_max_freq");
                if let Ok(text) = std::fs::read_to_string(&path)
                    && let Ok(freq) = text.trim().parse::<u64>()
                {
                    freqs.push((cpu, freq));
                }
            }
            let max_freq = freqs.iter().map(|&(_, f)| f).max().unwrap_or(0);
            if max_freq == 0 {
                return 0;
            }
            let threshold = (max_freq * 85) / 100;
            let mut mask = 0usize;
            for (cpu, freq) in freqs {
                if freq >= threshold {
                    mask |= 1usize << cpu;
                }
            }
            mask
        });
        if mask != 0 {
            unsafe extern "C" {
                fn sched_setaffinity(pid: i32, cpusetsize: usize, mask: *const usize) -> i32;
            }
            // SAFETY: pid 0 targets the calling thread; mask is a valid stack-allocated
            // CPU set word sized to std::mem::size_of::<usize>().
            unsafe {
                let _ = sched_setaffinity(0, std::mem::size_of::<usize>(), &mask);
            }
        }
    }
}

fn cpu_provider() -> ort::execution_providers::ExecutionProviderDispatch {
    CPUExecutionProvider::default().build().fail_silently()
}

fn provider_dispatch(
    provider: ExecutionProviderKind,
    device_id: u32,
    threads: usize,
) -> ort::execution_providers::ExecutionProviderDispatch {
    match provider {
        ExecutionProviderKind::Cpu => cpu_provider(),
        ExecutionProviderKind::Xnnpack => {
            let threads = std::num::NonZeroUsize::new(threads.max(1)).unwrap();
            XNNPACKExecutionProvider::default()
                .with_intra_op_num_threads(threads)
                .build()
                .fail_silently()
        },
        #[cfg(target_os = "windows")]
        ExecutionProviderKind::DirectMl => DirectMLExecutionProvider::default()
            .with_device_id(device_id as i32)
            .build()
            .fail_silently(),
        #[cfg(target_os = "windows")]
        ExecutionProviderKind::Cuda => {
            CUDAExecutionProvider::default().build().fail_silently()
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        ExecutionProviderKind::CoreMl => {
            CoreMLExecutionProvider::default().build().fail_silently()
        }
        #[cfg(not(target_os = "windows"))]
        ExecutionProviderKind::DirectMl | ExecutionProviderKind::Cuda => cpu_provider(),
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        ExecutionProviderKind::CoreMl => {
            let _ = device_id;
            cpu_provider()
        }
    }
}

pub struct OnnxSessionRunner {
    pool: Arc<OrtSessionPool>,
}
struct TensorRequest<'a> {
    model_path: &'a Path,
    input_name: &'a str,
    shape: &'a [i64],
    input: &'a mut [f32],
    destination: &'a mut Vec<f32>,
}

impl OnnxSessionRunner {
    pub fn new(pool: Arc<OrtSessionPool>) -> Self {
        Self { pool }
    }

    pub fn run_f32(
        &self,
        model_path: &Path,
        input_name: &str,
        shape: &[i64],
        input: &[f32],
    ) -> Result<Vec<f32>, InfrastructureError> {
        let mut input = input.to_vec();
        let mut output = Vec::new();
        self.run_f32_into(model_path, input_name, shape, &mut input, &mut output)?;
        Ok(output)
    }
    /// Synchronous borrowing keeps the caller's tensor allocation resident and
    /// reusable. The exclusive slice is never retained beyond this call.
    pub fn run_f32_into(
        &self,
        model_path: &Path,
        input_name: &str,
        shape: &[i64],
        input: &mut [f32],
        destination: &mut Vec<f32>,
    ) -> Result<(), InfrastructureError> {
        if input_name.trim().is_empty() {
            return Err(InfrastructureError::OnnxRuntime(
                "tensor input name cannot be empty".into(),
            ));
        }
        let element_count = shape
            .iter()
            .try_fold(1usize, |size, dimension| {
                if *dimension <= 0 {
                    return None;
                }
                usize::try_from(*dimension)
                    .ok()
                    .and_then(|dimension| size.checked_mul(dimension))
            })
            .ok_or_else(|| {
                InfrastructureError::OnnxRuntime(
                    "tensor shape must contain positive dimensions and fit usize".into(),
                )
            })?;
        if element_count != input.len() {
            return Err(InfrastructureError::OnnxRuntime(format!(
                "tensor shape describes {element_count} values, received {}",
                input.len()
            )));
        }
        if input.iter().any(|value| !value.is_finite()) {
            return Err(InfrastructureError::OnnxRuntime(
                "tensor input contains non-finite values".into(),
            ));
        }
        let mut request = TensorRequest {
            model_path,
            input_name,
            shape,
            input,
            destination,
        };
        crate::provider_retry::run_with_fallback(
            |force_cpu| {
                let mut provider = None;
                self.run_once(&mut request, force_cpu, &mut provider)
                    .map_err(|error| crate::provider_retry::AttemptFailure { error, provider })
            },
            |error| self.pool.quarantine(model_path, error),
        )
    }
    fn run_once(
        &self,
        request: &mut TensorRequest<'_>,
        force_cpu: bool,
        provider: &mut Option<ExecutionProviderKind>,
    ) -> Result<(), InfrastructureError> {
        let TensorRequest {
            model_path,
            input_name,
            shape,
            input,
            destination,
        } = request;
        let session = self.pool.load_session_mode(model_path, force_cpu)?;
        let session = session.lock().map_err(|_| {
            InfrastructureError::OnnxRuntime("ONNX session lock was poisoned".into())
        })?;
        let info = MemoryInfo::new(
            AllocationDevice::CPU,
            0,
            AllocatorType::Arena,
            MemoryType::CPUInput,
        )
        .map_err(|error| InfrastructureError::OnnxRuntime(error.to_string()))?;
        // SAFETY: dimensions are positive, their checked product equals the
        // exclusive, aligned f32 slice length, and therefore fits its allocation.
        // MemoryInfo describes CPU memory. Run is synchronous; neither the tensor
        // nor its inputs escape this function, and all ORT views drop before the
        // caller can reuse the slice. No asynchronous I/O binding is used.
        let tensor: TensorRefMut<'_, f32> =
            unsafe { TensorRefMut::from_raw(info, input.as_mut_ptr().cast(), shape.to_vec()) }
                .map_err(|error| InfrastructureError::OnnxRuntime(error.to_string()))?;
        promote_android_compute_thread();
        let inputs = vec![(*input_name, tensor)];
        let outputs = session.run(inputs).map_err(|error| {
            // Invalid models/arguments are permanent caller errors, not a
            // reason to retry work or disable an otherwise healthy GPU.
            if !matches!(
                error.code(),
                ort::ErrorCode::InvalidArgument
                    | ort::ErrorCode::InvalidGraph
                    | ort::ErrorCode::InvalidProtobuf
                    | ort::ErrorCode::NoSuchFile
                    | ort::ErrorCode::NoModel
            ) {
                *provider = Some(session.provider);
            }
            InfrastructureError::OnnxRuntime(error.to_string())
        })?;
        *provider = Some(session.provider);
        let (_, output) = outputs.iter().next().ok_or_else(|| {
            InfrastructureError::OnnxRuntime("ONNX model returned no output tensors".into())
        })?;
        let output = output
            .try_extract_tensor::<f32>()
            .map_err(|error| InfrastructureError::OnnxRuntime(error.to_string()))?;
        destination.resize(output.len(), 0.0);
        for (target, value) in destination.iter_mut().zip(output.iter()) {
            if !value.is_finite() {
                return Err(InfrastructureError::OnnxRuntime(
                    "ONNX model returned non-finite values".into(),
                ));
            }
            *target = *value;
        }
        if session.provider == ExecutionProviderKind::DirectMl {
            self.pool.observe_gpu_memory(&session.diagnostic_key);
        }
        Ok(())
    }
}


/// Hardware CPU count. On Android `available_parallelism` honours the affinity mask of the calling
/// thread, which `promote_android_compute_thread` narrows to the 4 performance cores; reading that
/// would halve the thread budget (4 cpus - 2 reserved = 2 threads).
pub fn logical_cpu_count() -> usize {
    #[cfg(target_os = "android")]
    {
        let online = (0..32usize)
            .filter(|cpu| std::path::Path::new(&format!("/sys/devices/system/cpu/cpu{cpu}")).exists())
            .count();
        if online > 0 {
            return online;
        }
    }
    std::thread::available_parallelism().map_or(1, usize::from)
}
