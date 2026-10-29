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
    execution_providers::{
        CPUExecutionProvider, CUDAExecutionProvider, CoreMLExecutionProvider,
        DirectMLExecutionProvider,
    },
    memory::{AllocationDevice, AllocatorType, MemoryInfo, MemoryType},
    session::{Session, builder::SessionBuilder},
    value::TensorRefMut,
};

use crate::errors::InfrastructureError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ExecutionProviderKind {
    DirectMl,
    Cuda,
    CoreMl,
    Cpu,
}

pub struct OrtSessionPool {
    sessions: SessionPool<Mutex<Session>, SessionKey>,
    provider_order: Vec<ExecutionProviderKind>,
    intra_threads: AtomicUsize,
    profiling_directory: Option<PathBuf>,
    playback_probe: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    diagnostics: Mutex<BTreeMap<String, SessionDiagnostic>>,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct SessionKey {
    path: PathBuf,
    threads: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionDiagnostic {
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
    pub fn idle_scope(self: &Arc<Self>) -> IdleSessionScope {
        IdleSessionScope { pool: self.clone() }
    }
    pub fn new(capacity: usize) -> Result<Self, InfrastructureError> {
        // Detection is not qualification. Accelerators require explicit opt-in.
        let provider_order = vec![ExecutionProviderKind::Cpu];
        let logical_cpus = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1);
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
        })
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

    pub fn effective_intra_threads(&self) -> usize {
        let requested = self.intra_threads();
        if self.playback_probe.as_ref().is_some_and(|probe| probe()) {
            let logical = std::thread::available_parallelism().map_or(1, usize::from);
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
        let logical = std::thread::available_parallelism().map_or(1, usize::from);
        let threads = atomusicator_application::compute_policy::ComputeBudget::for_profile(
            profile, logical, false,
        )
        .threads_per_inference();
        self.intra_threads.store(threads, Ordering::Release);
    }

    pub fn with_compute_profile(
        capacity: usize,
        profile: atomusicator_application::compute_policy::ComputeProfile,
    ) -> Result<Self, InfrastructureError> {
        let logical = std::thread::available_parallelism().map_or(1, usize::from);
        let threads = atomusicator_application::compute_policy::ComputeBudget::for_profile(
            profile, logical, false,
        )
        .threads_per_inference();
        Self::with_cpu_threads(capacity, threads)
    }

    pub fn with_provider_order(
        capacity: usize,
        provider_order: Vec<ExecutionProviderKind>,
    ) -> Result<Self, InfrastructureError> {
        if provider_order.last() != Some(&ExecutionProviderKind::Cpu) {
            return Err(InfrastructureError::InvalidManifest(
                "execution provider order must end with the CPU fallback".into(),
            ));
        }
        let mut pool = Self::new(capacity)?;
        pool.provider_order = provider_order;
        Ok(pool)
    }

    /// Explicit CPU configuration for controlled measurements; production keeps its default policy.
    pub fn with_cpu_threads(
        capacity: usize,
        intra_threads: usize,
    ) -> Result<Self, InfrastructureError> {
        let logical = std::thread::available_parallelism().map_or(1, usize::from);
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
    ) -> Result<Arc<Mutex<Session>>, InfrastructureError> {
        let canonical_path = model_path
            .canonicalize()
            .map_err(|error| InfrastructureError::io(model_path, error))?;
        let threads = self.effective_intra_threads();
        self.sessions.load(SessionKey { path: canonical_path.clone(), threads }, || {
            let started = Instant::now();
            let mut failures = Vec::new();
            for provider in self.provider_order.iter().copied() {
            let result = SessionBuilder::new()
                .and_then(|builder| match &self.profiling_directory {
                    Some(directory) => builder.with_profiling(directory.join(crate::audio_tools::unique_id())),
                    None => Ok(builder),
                })
                .and_then(|builder| builder.with_intra_threads(threads))
                .and_then(|builder| builder.with_inter_threads(1))
                .and_then(|builder| builder.with_parallel_execution(false))
                .and_then(|builder| builder.with_intra_op_spinning(false))
                .and_then(|builder| builder.with_inter_op_spinning(false))
                .and_then(|builder| {
                    builder.with_memory_pattern(provider != ExecutionProviderKind::DirectMl)
                })
                .and_then(|builder| builder.with_execution_providers([provider_dispatch(provider).error_on_failure()]))
                .and_then(|builder| builder.commit_from_file(&canonical_path));
            match result {
                Ok(session) => {
                    let diagnostic = SessionDiagnostic {
                        intra_threads: threads,
                        provider_registered: provider,
                        provider_effective: (provider == ExecutionProviderKind::Cpu).then(|| "CPUExecutionProvider".into()),
                        fallback_errors: failures,
                        initialization_seconds: started.elapsed().as_secs_f64(),
                    };
                    tracing::info!(provider = ?provider, seconds = diagnostic.initialization_seconds, fallback = ?diagnostic.fallback_errors, "ONNX session initialized");
                    if let Ok(mut diagnostics) = self.diagnostics.lock() {
                        // Bound telemetry independently of the model paths seen.
                        if diagnostics.len() >= 64 { diagnostics.clear(); }
                        diagnostics.insert(format!("{}#{threads}", canonical_path.display()), diagnostic);
                    }
                    return Ok(Mutex::new(session));
                }
                Err(error) => failures.push(format!("{provider:?}: {error}")),
            }
            }
            Err(failures.join("; "))
        })
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

fn provider_dispatch(
    provider: ExecutionProviderKind,
) -> ort::execution_providers::ExecutionProviderDispatch {
    match provider {
        ExecutionProviderKind::DirectMl => {
            DirectMLExecutionProvider::default().build().fail_silently()
        }
        ExecutionProviderKind::Cuda => CUDAExecutionProvider::default().build().fail_silently(),
        ExecutionProviderKind::CoreMl => CoreMLExecutionProvider::default().build().fail_silently(),
        ExecutionProviderKind::Cpu => CPUExecutionProvider::default().build().fail_silently(),
    }
}

pub struct OnnxSessionRunner {
    pool: Arc<OrtSessionPool>,
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
        let session = self.pool.load_session(model_path)?;
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
        let inputs = vec![(input_name, tensor)];
        let outputs = session
            .run(inputs)
            .map_err(|error| InfrastructureError::OnnxRuntime(error.to_string()))?;
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
        Ok(())
    }
}
