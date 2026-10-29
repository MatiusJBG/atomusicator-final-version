//! Observe output callback sizing using silence, without recording system audio.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
fn main() {
    let device = cpal::default_host()
        .default_output_device()
        .expect("output device");
    let supported = device.default_output_config().unwrap();
    println!(
        "format={:?}, rate={}, supported_buffer={:?}",
        supported.sample_format(),
        supported.sample_rate().0,
        supported.buffer_size()
    );
    for fixed in [None, Some(512)] {
        let mut config = supported.config();
        if let Some(frames) = fixed {
            config.buffer_size = cpal::BufferSize::Fixed(frames);
        }
        let maximum = Arc::new(AtomicUsize::new(0));
        let capture = maximum.clone();
        let channels = config.channels as usize;
        let stream = device.build_output_stream(
            &config,
            move |samples: &mut [f32], _| {
                samples.fill(0.0);
                capture.fetch_max(samples.len() / channels, Ordering::Relaxed);
            },
            |e| eprintln!("{e}"),
            None,
        );
        match stream {
            Ok(stream) => {
                stream.play().unwrap();
                std::thread::sleep(std::time::Duration::from_millis(350));
                drop(stream);
                println!(
                    "requested={fixed:?}, observed_callback_frames={}",
                    maximum.load(Ordering::Acquire)
                );
            }
            Err(error) => println!("requested={fixed:?}, error={error}"),
        }
    }
}
