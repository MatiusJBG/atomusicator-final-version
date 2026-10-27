use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use lru::LruCache;
use ort::{
    execution_providers::{
        CPUExecutionProvider, CUDAExecutionProvider, CoreMLExecutionProvider,
        DirectMLExecutionProvider,
    },
    session::{Session, builder::SessionBuilder},
    value::Tensor,
};

use crate::errors::InfrastructureError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionProviderKind {
    DirectMl,
    Cuda,
    CoreMl,
    Cpu,
}

pub struct OrtSessionPool {
    sessions: Mutex<LruCache<PathBuf, Arc<Mutex<Session>>>>,
    provider_order: Vec<ExecutionProviderKind>,
    intra_threads: usize,
}

impl OrtSessionPool {
    pub fn new(capacity: usize) -> Result<Self, InfrastructureError> {
        let capacity = NonZeroUsize::new(capacity).ok_or_else(|| {
            InfrastructureError::InvalidManifest(
                "ONNX session pool capacity must be positive".into(),
            )
        })?;
        // Detection is not qualification. Accelerators require explicit opt-in.
        let provider_order = vec![ExecutionProviderKind::Cpu];
        let logical_cpus = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1);
        let intra_threads =
            atomusicator_application::compute_policy::ComputeBudget::interactive(logical_cpus)
                .threads_per_inference();
        Ok(Self {
            sessions: Mutex::new(LruCache::new(capacity)),
            provider_order,
            intra_threads,
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

    pub fn load_session(
        &self,
        model_path: &Path,
    ) -> Result<Arc<Mutex<Session>>, InfrastructureError> {
        let canonical_path = model_path
            .canonicalize()
            .map_err(|error| InfrastructureError::io(model_path, error))?;
        // Serialize admission as well as lookup: concurrent misses must not load
        // duplicate heavyweight models. No session lease is retained by runners.
        let mut sessions = self.sessions.lock().map_err(|_| {
            InfrastructureError::OnnxRuntime("session pool lock was poisoned".into())
        })?;
        if let Some(session) = sessions.get(&canonical_path).cloned() {
            return Ok(session);
        }
        if sessions.len() == sessions.cap().get() {
            sessions.pop_lru();
        }

        let providers = self
            .provider_order
            .iter()
            .copied()
            .map(provider_dispatch)
            .collect::<Vec<_>>();
        let session = SessionBuilder::new()
            .and_then(|builder| builder.with_intra_threads(self.intra_threads))
            .and_then(|builder| builder.with_inter_threads(1))
            .and_then(|builder| builder.with_parallel_execution(false))
            .and_then(|builder| builder.with_intra_op_spinning(false))
            .and_then(|builder| builder.with_inter_op_spinning(false))
            .and_then(|builder| {
                builder.with_memory_pattern(
                    !self
                        .provider_order
                        .contains(&ExecutionProviderKind::DirectMl),
                )
            })
            .and_then(|builder| builder.with_execution_providers(providers))
            .and_then(|builder| builder.commit_from_file(&canonical_path))
            .map_err(|error| InfrastructureError::OnnxRuntime(error.to_string()))?;
        let session = Arc::new(Mutex::new(session));
        sessions.put(canonical_path, session.clone());
        Ok(session)
    }

    pub fn provider_order(&self) -> &[ExecutionProviderKind] {
        &self.provider_order
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
