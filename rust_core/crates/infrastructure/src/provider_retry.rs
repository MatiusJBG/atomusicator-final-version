use crate::{errors::InfrastructureError, onnx::ExecutionProviderKind};

pub(crate) struct AttemptFailure {
    pub error: InfrastructureError,
    pub provider: Option<ExecutionProviderKind>,
}
pub(crate) fn run_with_fallback(
    mut attempt: impl FnMut(bool) -> Result<(), AttemptFailure>,
    mut quarantine: impl FnMut(&InfrastructureError) -> Result<(), InfrastructureError>,
) -> Result<(), InfrastructureError> {
    match attempt(false) {
        Ok(()) => Ok(()),
        Err(failed) => {
            if failed.provider.is_none() || failed.provider == Some(ExecutionProviderKind::Cpu) {
                return Err(failed.error);
            }
            if let Err(admission) = quarantine(&failed.error) {
                return Err(InfrastructureError::OnnxRuntime(format!(
                    "accelerator Run failed ({}); CPU fallback not admitted ({admission})",
                    failed.error
                )));
            }
            attempt(true).map_err(|cpu| {
                InfrastructureError::OnnxRuntime(format!(
                    "accelerator Run failed ({}); CPU fallback failed ({})",
                    failed.error, cpu.error
                ))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accelerator_failure_retries_once_and_cpu_failures_do_not_loop() {
        let mut calls = Vec::new();
        let mut quarantine_count = 0;
        let result = run_with_fallback(
            |cpu| {
                calls.push(cpu);
                Err(AttemptFailure {
                    error: InfrastructureError::OnnxRuntime(
                        if cpu { "cpu-error" } else { "gpu-error" }.into(),
                    ),
                    provider: Some(if cpu {
                        ExecutionProviderKind::Cpu
                    } else {
                        ExecutionProviderKind::DirectMl
                    }),
                })
            },
            |_| {
                quarantine_count += 1;
                Ok(())
            },
        );
        assert_eq!(calls, vec![false, true]);
        assert_eq!(quarantine_count, 1);
        let error = result.unwrap_err().to_string();
        assert!(error.contains("gpu-error") && error.contains("cpu-error"));
        let mut calls = 0;
        assert!(
            run_with_fallback(
                |_| {
                    calls += 1;
                    Err(AttemptFailure {
                        error: InfrastructureError::OnnxRuntime("bad input".into()),
                        provider: None,
                    })
                },
                |_| panic!("validation must not quarantine")
            )
            .is_err()
        );
        assert_eq!(calls, 1);
    }
}
