//! Concrete adapters live here; domain policy remains in the inner crates.

pub mod audio_evidence;
pub mod audio_io;
pub mod audio_tools;
pub mod block_workspace;
pub mod cache;
pub mod dsp;
pub mod errors;
pub mod job_journal;
pub mod mdx;
pub mod mdx_bounded;
pub mod model_downloader;
pub mod models;
pub mod onnx;
pub mod pitch_cache;
pub mod preferences;
pub mod resource_probe;
pub mod session_reader;
pub mod session_workspace;
pub mod storage;
pub mod use_case_adapters;
pub mod waveform;
pub mod waveform_index;
