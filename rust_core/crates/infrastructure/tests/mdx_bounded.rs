use atomusicator_application::{CancellationToken, ports::AudioBlockStore};
use atomusicator_domain::{AudioAssetMetadata, AudioFormat};
use atomusicator_infrastructure::{
    audio_tools::TemporaryDirectory,
    mdx::{MdxConfig, MdxTransform},
    mdx_bounded::run_bounded_mdx,
    storage::float_spool::FloatSpoolStore,
};

// Independent whole-file accumulator mirrors the frozen implementation's operation order.
fn reference(samples: &[f32], c: MdxConfig) -> Vec<f32> {
    let frames = samples.len() / 2;
    let chunk = c.chunk_frames();
    let trim = c.n_fft / 2;
    let generation = chunk - 2 * trim;
    let padded = trim + frames + generation + trim - frames % generation;
    let step = (0.65 * chunk as f64) as usize;
    let peak = samples.iter().map(|s| s.abs()).fold(0.0, f32::max);
    let transform = MdxTransform::new(c).unwrap();
    let mut out = vec![0.0; padded * 2];
    let mut weights = vec![0.0; padded];
    for start in (0..padded).step_by(step) {
        let actual = chunk.min(padded - start);
        let mut input = vec![0.0; chunk * 2];
        for frame in 0..actual {
            let p = start + frame;
            if p >= trim && p - trim < frames {
                input[frame * 2] = samples[(p - trim) * 2] / peak;
                input[frame * 2 + 1] = samples[(p - trim) * 2 + 1] / peak;
            }
        }
        let mut spectrum = transform.forward(&input).unwrap();
        for component in 0..4 {
            spectrum[component * c.dim_f * c.dim_t..(component * c.dim_f + 3) * c.dim_t].fill(0.0);
        }
        for s in &mut spectrum {
            *s *= 0.6;
        }
        let decoded = transform.inverse(&spectrum).unwrap();
        for frame in 0..actual {
            let weight = if actual > 1 {
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * frame as f32 / (actual - 1) as f32).cos()
            } else {
                1.0
            };
            weights[start + frame] += weight;
            for channel in 0..2 {
                out[(start + frame) * 2 + channel] += decoded[frame * 2 + channel] * weight;
            }
        }
    }
    (0..frames * 2)
        .map(|i| {
            let value = out[trim * 2 + i] / weights[trim + i / 2] * peak * c.compensate;
            if c.invert { samples[i] - value } else { value }
        })
        .collect()
}
#[test]
fn rolling_overlap_is_bit_identical_to_whole_file_accumulation_at_boundaries() {
    let root = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
    let store = FloatSpoolStore::new(root.0.clone());
    for frames in [1, 31, 59, 60, 61, 117, 120, 121, 178, 4097] {
        for invert in [false, true] {
            let c = MdxConfig {
                n_fft: 60,
                hop: 12,
                dim_f: 31,
                dim_t: 16,
                compensate: 1.043,
                invert,
            };
            let samples: Vec<f32> = (0..frames * 2)
                .map(|i| (i as f32 * 0.173).sin() * if i < frames { 0.13 } else { 0.8 })
                .collect();
            let meta = AudioAssetMetadata {
                format: AudioFormat::new(44100, 2).unwrap(),
                frame_count: frames as u64,
            };
            let mut input = store.create("input", meta).unwrap();
            input.write(&samples).unwrap();
            let input = input.finish().unwrap();
            let mut output = store.create("output", meta).unwrap();
            run_bounded_mdx(
                c,
                input.as_ref(),
                output.as_mut(),
                &CancellationToken::default(),
                |s| Ok(s.iter().map(|v| v * 0.6).collect()),
            )
            .unwrap();
            let output = output.finish().unwrap();
            let mut actual = vec![0.0; samples.len()];
            output.read_frames(0, &mut actual).unwrap();
            assert_eq!(
                actual,
                reference(&samples, c),
                "frames={frames} invert={invert}"
            );
        }
    }
}
