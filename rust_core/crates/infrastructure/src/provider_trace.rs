//! Evidence of actual ORT kernel assignment, independent of provider registration.
use crate::errors::InfrastructureError;
use std::{collections::BTreeMap, io::Read, path::Path};

const MAX_TRACE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, serde::Serialize)]
pub struct ProviderAssignment {
    pub kernel_events: BTreeMap<String, u64>,
}
impl ProviderAssignment {
    pub fn from_file(path: &Path) -> Result<Self, InfrastructureError> {
        let file = std::fs::File::open(path).map_err(|e| InfrastructureError::io(path, e))?;
        let mut bytes = Vec::new();
        file.take(MAX_TRACE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| InfrastructureError::io(path, e))?;
        Self::from_json(&bytes)
    }
    pub fn from_json(bytes: &[u8]) -> Result<Self, InfrastructureError> {
        if bytes.len() as u64 > MAX_TRACE_BYTES {
            return Err(InfrastructureError::OnnxRuntime(
                "provider trace exceeds 16 MiB".into(),
            ));
        }
        let events: Vec<serde_json::Value> = serde_json::from_slice(bytes).map_err(|e| {
            InfrastructureError::OnnxRuntime(format!("invalid provider trace: {e}"))
        })?;
        let mut kernel_events = BTreeMap::new();
        for event in events {
            if event["cat"] == "Node"
                && event["name"]
                    .as_str()
                    .is_some_and(|name| name.ends_with("_kernel_time"))
                && let Some(provider) = event["args"]["provider"].as_str()
            {
                *kernel_events.entry(provider.to_owned()).or_insert(0) += 1;
            }
        }
        Ok(Self { kernel_events })
    }
    pub fn directml_executed(&self) -> bool {
        self.kernel_events
            .get("DmlExecutionProvider")
            .copied()
            .unwrap_or(0)
            > 0
    }
}
