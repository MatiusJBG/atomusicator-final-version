//! Opt-in aggregate observations. Tensor magnitudes use spectral units and must
//! never be interpreted as PCM energy or compared directly with reference RMS.
use serde::Serialize;
use std::{collections::BTreeMap, sync::Mutex};
#[derive(Default, Clone, Serialize)]
pub struct Level {
    pub samples: u64,
    pub calls: u64,
    pub energy_sum: f64,
    pub peak: f32,
}
#[derive(Default)]
pub struct InferenceDiagnostics {
    levels: Mutex<BTreeMap<String, Level>>,
}
impl InferenceDiagnostics {
    pub fn record(&self, key: &str, values: &[f32]) {
        let energy = values.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
        let peak = values.iter().map(|v| v.abs()).fold(0.0, f32::max);
        if let Ok(mut levels) = self.levels.lock() {
            let level = levels.entry(key.to_owned()).or_default();
            level.calls += 1;
            level.samples += values.len() as u64;
            level.energy_sum += energy;
            level.peak = level.peak.max(peak);
        }
    }
    pub fn report(&self) -> serde_json::Value {
        match self.levels.lock() {
            Ok(levels) => {
                serde_json::json!({"scope":"window observations include padding and overlap; tensor entries are spectral units, not PCM; normalized-window and whole-track RMS are not directly comparable",
                "levels":levels.iter().map(|(key,v)|(key,serde_json::json!({"samples":v.samples,"calls":v.calls,"energy_sum":v.energy_sum,"rms":(v.energy_sum/v.samples.max(1) as f64).sqrt(),"peak":v.peak}))).collect::<BTreeMap<_,_>>() })
            }
            Err(_) => serde_json::json!({"error":"diagnostic lock poisoned"}),
        }
    }
}
