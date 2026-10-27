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
}

impl OrtSessionPool {
    pub fn new(capacity: usize) -> Result<Self, InfrastructureError> {
        let capacity = NonZeroUsize::new(capacity).ok_or_else(|| {
            InfrastructureError::InvalidManifest(
                "ONNX session pool capacity must be positive".into(),
            )
        })?;
        let mut provider_order = Vec::new();
        #[cfg(target_os = "windows")]
        provider_order.extend([ExecutionProviderKind::DirectMl, ExecutionProviderKind::Cuda]);
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        provider_order.push(ExecutionProviderKind::CoreMl);
        #[cfg(target_os = "linux")]
        provider_order.push(ExecutionProviderKind::Cuda);
        provider_order.push(ExecutionProviderKind::Cpu);
        Ok(Self {
            sessions: Mutex::new(LruCache::new(capacity)),
            provider_order,
        })
    }

    pub fn with_provider_order(
        capacity: usize,
        provider_order: Vec<ExecutionProviderKind>,
    ) -> Result<Self, InfrastructureError> {
        if provider_order.is_empty() || !provider_order.contains(&ExecutionProviderKind::Cpu) {
            return Err(InfrastructureError::InvalidManifest(
                "execution provider order must include CPU fallback".into(),
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
        if let Some(session) = self
            .sessions
            .lock()
            .map_err(|_| InfrastructureError::OnnxRuntime("session pool lock was poisoned".into()))?
            .get(&canonical_path)
            .cloned()
        {
            return Ok(session);
        }

        let providers = self
            .provider_order
            .iter()
            .copied()
            .map(provider_dispatch)
            .collect::<Vec<_>>();
        let session = SessionBuilder::new()
            .and_then(|builder| builder.with_execution_providers(providers))
            .and_then(|builder| builder.commit_from_file(&canonical_path))
            .map_err(|error| InfrastructureError::OnnxRuntime(error.to_string()))?;
        let session = Arc::new(Mutex::new(session));
        let mut sessions = self.sessions.lock().map_err(|_| {
            InfrastructureError::OnnxRuntime("session pool lock was poisoned".into())
        })?;
        if let Some(existing) = sessions.get(&canonical_path).cloned() {
            return Ok(existing);
        }
        sessions.put(canonical_path, session.clone());
        Ok(session)
    }

    pub fn provider_order(&self) -> &[ExecutionProviderKind] {
        &self.provider_order
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
        let element_count = shape
            .iter()
            .try_fold(1usize, |size, dimension| {
                usize::try_from(*dimension)
                    .ok()
                    .and_then(|dimension| size.checked_mul(dimension))
            })
            .ok_or_else(|| {
                InfrastructureError::OnnxRuntime(
                    "tensor shape is invalid or overflows usize".into(),
                )
            })?;
        if element_count != input.len() {
            return Err(InfrastructureError::OnnxRuntime(format!(
                "tensor shape describes {element_count} values, received {}",
                input.len()
            )));
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
        let output = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|error| InfrastructureError::OnnxRuntime(error.to_string()))?;
        Ok(output.iter().copied().collect())
    }
}
