use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};

use crate::session_pool::SessionPool;
use ort::{
    execution_providers::{
        CPUExecutionProvider, CUDAExecutionProvider, CoreMLExecutionProvider,
        DirectMLExecutionProvider,
    },
    session::{Session, builder::SessionBuilder},
    value::Tensor,
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
    sessions: SessionPool<Mutex<Session>>,
    provider_order: Vec<ExecutionProviderKind>,
    intra_threads: usize,
    diagnostics: Mutex<BTreeMap<String, SessionDiagnostic>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionDiagnostic {
    pub provider_registered: ExecutionProviderKind,
    /// CPU-only sessions have an unambiguous assignment. Accelerator sessions
    /// may partition nodes onto CPU; registration alone cannot prove assignment.
    pub provider_effective: Option<String>,
    pub fallback_errors: Vec<String>,
    pub initialization_seconds: f64,
}

impl OrtSessionPool {
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
            intra_threads,
            diagnostics: Mutex::new(BTreeMap::new()),
        })
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
        pool.intra_threads = intra_threads;
        Ok(pool)
    }

    pub fn load_session(
        &self,
        model_path: &Path,
    ) -> Result<Arc<Mutex<Session>>, InfrastructureError> {
        let canonical_path = model_path
            .canonicalize()
            .map_err(|error| InfrastructureError::io(model_path, error))?;
        self.sessions.load(canonical_path.clone(), || {
            let started = Instant::now();
            let mut failures = Vec::new();
            for provider in self.provider_order.iter().copied() {
            let result = SessionBuilder::new()
                .and_then(|builder| builder.with_intra_threads(self.intra_threads))
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
                        provider_registered: provider,
                        provider_effective: (provider == ExecutionProviderKind::Cpu).then(|| "CPUExecutionProvider".into()),
                        fallback_errors: failures,
                        initialization_seconds: started.elapsed().as_secs_f64(),
                    };
                    tracing::info!(provider = ?provider, seconds = diagnostic.initialization_seconds, fallback = ?diagnostic.fallback_errors, "ONNX session initialized");
                    if let Ok(mut diagnostics) = self.diagnostics.lock() {
                        // Bound telemetry independently of the model paths seen.
                        if diagnostics.len() >= 64 { diagnostics.clear(); }
                        diagnostics.insert(canonical_path.to_string_lossy().into_owned(), diagnostic);
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
        self.intra_threads
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
        let tensor: Tensor<f32> = (shape.to_vec(), input.to_vec())
            .try_into()
            .map_err(|error: ort::Error| InfrastructureError::OnnxRuntime(error.to_string()))?;
        let inputs = ort::inputs! { input_name => tensor }
            .map_err(|error| InfrastructureError::OnnxRuntime(error.to_string()))?;
        let outputs = session
            .run(inputs)
            .map_err(|error| InfrastructureError::OnnxRuntime(error.to_string()))?;
        let (_, output) = outputs.iter().next().ok_or_else(|| {
            InfrastructureError::OnnxRuntime("ONNX model returned no output tensors".into())
        })?;
        let output = output
            .try_extract_tensor::<f32>()
            .map_err(|error| InfrastructureError::OnnxRuntime(error.to_string()))?;
        if output.iter().any(|value| !value.is_finite()) {
            return Err(InfrastructureError::OnnxRuntime(
                "ONNX model returned non-finite values".into(),
            ));
        }
        Ok(output.iter().copied().collect())
    }
}
