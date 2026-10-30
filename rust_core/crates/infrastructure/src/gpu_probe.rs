//! DXGI budgets belong to the calling process; they are not global GPU usage.
use crate::errors::InfrastructureError;
#[derive(Debug, Clone, serde::Serialize)]
pub struct AdapterMemory {
    pub index: u32,
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub software: bool,
    pub dedicated_bytes: u64,
    pub shared_bytes: u64,
    pub budget_bytes: Option<u64>,
    pub current_process_usage_bytes: Option<u64>,
}
#[cfg(windows)]
#[repr(C)]
struct NativeMemory {
    index: u32,
    vendor: u32,
    device: u32,
    software: u32,
    dedicated: u64,
    shared: u64,
    budget: u64,
    usage: u64,
    budget_supported: u32,
    name: [u16; 128],
}
#[cfg(windows)]
unsafe extern "C" {
    fn atomusicator_gpu_memory(index: u32, output: *mut NativeMemory) -> i32;
}

pub fn adapter(index: u32) -> Result<Option<AdapterMemory>, InfrastructureError> {
    #[cfg(windows)]
    {
        let mut info: NativeMemory = unsafe { std::mem::zeroed() };
        // SAFETY: repr(C) matches the native fixed-size POD. The synchronous
        // function writes this valid exclusive buffer and retains no pointer.
        let result = unsafe { atomusicator_gpu_memory(index, &mut info) };
        if result == 1 {
            return Ok(None);
        }
        if result != 0 {
            return Err(InfrastructureError::OnnxRuntime(format!(
                "DXGI observation failed: 0x{:08x}",
                result as u32
            )));
        }
        let end = info
            .name
            .iter()
            .position(|v| *v == 0)
            .unwrap_or(info.name.len());
        Ok(Some(AdapterMemory {
            index: info.index,
            name: String::from_utf16_lossy(&info.name[..end]),
            vendor_id: info.vendor,
            device_id: info.device,
            software: info.software != 0,
            dedicated_bytes: info.dedicated,
            shared_bytes: info.shared,
            budget_bytes: (info.budget_supported != 0).then_some(info.budget),
            current_process_usage_bytes: (info.budget_supported != 0).then_some(info.usage),
        }))
    }
    #[cfg(not(windows))]
    {
        let _ = index;
        Ok(None)
    }
}
