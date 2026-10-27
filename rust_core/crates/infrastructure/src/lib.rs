//! Concrete adapters live here; domain policy remains in the inner crates.

pub mod audio_io;
pub mod cache;
pub mod dsp;
pub mod errors;
pub mod model_downloader;
pub mod onnx;
pub mod session_workspace;
pub mod use_case_adapters;
pub mod waveform;
