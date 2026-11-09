//! Current-process RSS, separate from system availability and process-tree samples.
pub fn resident_bytes() -> Option<u64> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::{
            ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
            Threading::GetCurrentProcess,
        };
        // SAFETY: the Windows POD is integer-only, initialized to zero and passed
        // exclusively with its exact size; the pseudo handle is never closed.
        let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        counters.cb = size;
        (unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) } != 0)
            .then_some(counters.WorkingSetSize as u64)
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        std::fs::read_to_string("/proc/self/status")
            .ok()?
            .lines()
            .find_map(|line| {
                line.strip_prefix("VmRSS:")
                    .and_then(|v| v.split_whitespace().next())
                    .and_then(|v| v.parse::<u64>().ok())
            })
            .and_then(|v| v.checked_mul(1024))
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "android")))]
    {
        None
    }
}
