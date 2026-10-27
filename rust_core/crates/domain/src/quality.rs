//! Serializable semantic identity. Operational provider/thread identity is separate.
use serde::{Deserialize, Serialize};
pub const RECONSTRUCTION_TOLERANCE: f32 = 1e-5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelTransform {
    pub profile_id: String,
    pub n_fft: usize,
    pub hop: usize,
    pub dim_f: usize,
    pub dim_t: usize,
    pub compensate: f32,
    pub invert: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeparationProfile {
    pub schema_version: u32,
    pub id: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub models: Vec<ModelTransform>,
    pub tensor_layout: String,
    pub window: String,
    pub centered_padding: String,
    pub input_normalization: String,
    pub overlap: f64,
    pub zero_low_bins: usize,
    pub vad_seconds: Option<u32>,
    pub vad_dbfs: Option<f64>,
    pub bleed_gate_fft: Option<usize>,
    pub bleed_gate_floor: Option<f32>,
    pub bleed_gate_ratio: Option<f32>,
    pub rhythm_dependency: String,
    pub reconstruction_tolerance: f32,
}
impl SeparationProfile {
    pub fn compatibility() -> Self {
        let models = [
            ("uvr-mdxnet-main-v1", 7680, 3072, 256, 1.043, false),
            ("uvr-mdxnet-kara-2-v1", 5120, 2048, 256, 1.065, true),
            ("kuielab-mdxnet-drums-v1", 4096, 2048, 512, 1.035, false),
            ("kuielab-mdxnet-bass-v1", 16384, 2048, 512, 1.035, false),
        ]
        .into_iter()
        .map(
            |(id, n_fft, dim_f, dim_t, compensate, invert)| ModelTransform {
                profile_id: id.into(),
                n_fft,
                hop: 1024,
                dim_f,
                dim_t,
                compensate,
                invert,
            },
        )
        .collect();
        Self {
            schema_version: 1,
            id: "native-v1".into(),
            sample_rate: 44100,
            channels: 2,
            models,
            tensor_layout: "batch,L-real,L-imag,R-real,R-imag;frequency,time".into(),
            window: "periodic-Hann;OLA=symmetric-Hann-actual-window".into(),
            centered_padding: "reflect;global-zero-pad;grid-origin=0".into(),
            input_normalization: "global-input-peak-per-model-call;restore-input-peak".into(),
            overlap: 0.35,
            zero_low_bins: 3,
            vad_seconds: Some(8),
            vad_dbfs: Some(-65.0),
            bleed_gate_fft: Some(2048),
            bleed_gate_floor: Some(0.05),
            bleed_gate_ratio: Some(0.25),
            rhythm_dependency: "mix-active-vocal-bus;drums-then-bass-on-drum-free-residual".into(),
            reconstruction_tolerance: RECONSTRUCTION_TOLERANCE,
        }
    }
    pub fn context_candidate() -> Self {
        Self {
            id: "context-candidate-v1".into(),
            vad_seconds: None,
            vad_dbfs: None,
            bleed_gate_fft: None,
            bleed_gate_floor: None,
            bleed_gate_ratio: None,
            ..Self::compatibility()
        }
    }
}
