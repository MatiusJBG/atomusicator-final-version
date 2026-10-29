//! Release-build native DSP qualification. No physical output or latency claim.
use atomusicator_domain::PitchShiftSpec;
use atomusicator_infrastructure::live_pitch::LivePitchProcessor;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let window = args.get(1).expect("window").parse().unwrap();
    let hop = args.get(2).expect("hop").parse().unwrap();
    let mut engines: Vec<_> = (0..5)
        .map(|_| LivePitchProcessor::with_profile(48000, window, hop).unwrap())
        .collect();
    let mut timings = Vec::new();
    let mut output = [0.0; 512];
    for block in 0..1000 {
        let input: Vec<_> = (0..256)
            .flat_map(|i| {
                let v = (std::f32::consts::TAU * 440.0 * (block * 256 + i) as f32 / 48000.0).sin()
                    * 0.2;
                [v, v * 0.5]
            })
            .collect();
        let started = std::time::Instant::now();
        for engine in &mut engines {
            engine.set_spec(
                PitchShiftSpec::try_new(if block % 200 < 100 { 7.0 } else { -7.0 }, true).unwrap(),
            );
            engine.process(&input, &mut output).unwrap();
        }
        if block > 100 {
            timings.push(started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    timings.sort_by(f64::total_cmp);
    println!(
        "{{\"window\":{window},\"hop\":{hop},\"five_stems\":true,\"formants\":true,\"rtf\":{},\"p95_ms\":{},\"max_ms\":{},\"output_delay_ms\":{}}}",
        timings.iter().sum::<f64>() / timings.len() as f64 / (256.0 / 48.0),
        timings[timings.len() * 95 / 100],
        timings[timings.len() - 1],
        engines[0].output_delay_frames() as f64 / 48.0
    );
}
