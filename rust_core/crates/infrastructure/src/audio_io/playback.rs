//! CPAL output with callback-owned track readers and lock-free realtime controls.

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc::{self, Sender, SyncSender},
    },
    thread::{self, JoinHandle},
};

use super::streamer::{BufferedStreamTrackSource, StreamProducerGuard};
use atomusicator_application::ports::RealtimePlaybackPort;
use atomusicator_domain::{DomainError, PitchShiftSpec, Stem, StemType};
use cpal::{
    FromSample, Sample, SampleFormat as CpalSampleFormat, SizedSample,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

const TRACK_COUNT: usize = 5;

struct Transport {
    playing: AtomicBool,
    position: AtomicU64,
    master: AtomicU32,
    peak: AtomicU32,
}
struct MixerControls {
    tracks: [TrackControl; TRACK_COUNT],
    transport: Transport,
}
impl MixerControls {
    fn new() -> Self {
        Self {
            tracks: std::array::from_fn(|_| TrackControl::new()),
            transport: Transport {
                playing: AtomicBool::new(true),
                position: AtomicU64::new(0),
                master: AtomicU32::new(1.0f32.to_bits()),
                peak: AtomicU32::new(0),
            },
        }
    }
}
impl std::ops::Deref for MixerControls {
    type Target = [TrackControl; TRACK_COUNT];
    fn deref(&self) -> &Self::Target {
        &self.tracks
    }
}

struct TrackControl {
    gain: AtomicU32,
    pan_left: AtomicU32,
    pan_right: AtomicU32,
    mute: AtomicBool,
    solo: AtomicBool,
}

impl TrackControl {
    fn new() -> Self {
        Self {
            gain: AtomicU32::new(1.0f32.to_bits()),
            pan_left: AtomicU32::new(std::f32::consts::FRAC_1_SQRT_2.to_bits()),
            pan_right: AtomicU32::new(std::f32::consts::FRAC_1_SQRT_2.to_bits()),
            mute: AtomicBool::new(false),
            solo: AtomicBool::new(false),
        }
    }
}

#[derive(Clone)]
enum PlaybackSession {
    Memory(Arc<[Stem]>),
    Streaming(Arc<[(StemType, PathBuf)]>),
}

struct PreparedSession {
    playback: PlaybackSession,
}

enum MixerCommand {
    Start {
        session: PlaybackSession,
        controls: Arc<MixerControls>,
        pitch_ratio: Arc<AtomicU32>,
        reply: SyncSender<Result<(), String>>,
    },
    Stop(SyncSender<()>),
    Shutdown,
}

/// The CPAL stream stays on its creating control thread because CPAL streams
/// are not `Send` on all supported platforms. The application-facing adapter
/// itself remains `Send + Sync` through a message channel.
pub struct CpalRealtimeMixer {
    session: Mutex<Option<PreparedSession>>,
    controls: Arc<MixerControls>,
    pitch_ratio: Arc<AtomicU32>,
    commands: Sender<MixerCommand>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl CpalRealtimeMixer {
    pub fn new() -> Result<Self, DomainError> {
        let (commands, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("atomusicator-cpal-control".into())
            .spawn(move || run_device_worker(receiver))
            .map_err(|error| DomainError::DspFailure(error.to_string()))?;
        Ok(Self {
            session: Mutex::new(None),
            controls: Arc::new(MixerControls::new()),
            pitch_ratio: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            commands,
            worker: Mutex::new(Some(worker)),
        })
    }

    pub fn start(&self) -> Result<(), DomainError> {
        let session = self
            .session
            .lock()
            .map_err(|_| DomainError::DspFailure("audio session state was poisoned".into()))?;
        let session = session.as_ref().ok_or_else(|| {
            DomainError::DspFailure("load a complete stem session before playback".into())
        })?;
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(MixerCommand::Start {
                session: session.playback.clone(),
                controls: Arc::clone(&self.controls),
                pitch_ratio: Arc::clone(&self.pitch_ratio),
                reply,
            })
            .map_err(|error| DomainError::DspFailure(error.to_string()))?;
        let result = response
            .recv()
            .map_err(|error| DomainError::DspFailure(error.to_string()))?
            .map_err(DomainError::DspFailure);
        if result.is_ok() {
            self.controls
                .transport
                .playing
                .store(true, Ordering::Release);
        }
        result
    }

    pub fn pause(&self) {
        self.controls
            .transport
            .playing
            .store(false, Ordering::Release);
    }
    pub fn position_seconds(&self) -> f64 {
        f64::from_bits(self.controls.transport.position.load(Ordering::Acquire))
    }
    pub fn is_playing(&self) -> bool {
        self.controls.transport.playing.load(Ordering::Acquire)
    }
    pub fn peak(&self) -> f32 {
        f32::from_bits(self.controls.transport.peak.load(Ordering::Relaxed))
    }
    pub fn set_master(&self, gain: f32) -> Result<(), DomainError> {
        if !gain.is_finite() || !(0.0..=2.0).contains(&gain) {
            return Err(DomainError::DspFailure("invalid master gain".into()));
        }
        self.controls
            .transport
            .master
            .store(gain.to_bits(), Ordering::Relaxed);
        Ok(())
    }
    pub fn seek(&self, seconds: f64) -> Result<(), DomainError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(DomainError::DspFailure("invalid seek position".into()));
        }
        let session = self
            .session
            .lock()
            .map_err(|_| DomainError::DspFailure("session poisoned".into()))?;
        let Some(PreparedSession {
            playback: PlaybackSession::Memory(stems),
        }) = session.as_ref()
        else {
            return Err(DomainError::DspFailure(
                "seek requires a memory-backed session".into(),
            ));
        };
        let duration =
            stems[0].audio.frames() as f64 / f64::from(stems[0].audio.format().sample_rate_hz);
        self.controls
            .transport
            .position
            .store(seconds.min(duration).to_bits(), Ordering::Release);
        Ok(())
    }

    /// Stops and releases the stream on its owning control thread.
    pub fn stop(&self) -> Result<(), DomainError> {
        self.pause();
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(MixerCommand::Stop(reply))
            .map_err(|error| DomainError::DspFailure(error.to_string()))?;
        response
            .recv()
            .map_err(|error| DomainError::DspFailure(error.to_string()))?;
        self.controls.transport.position.store(0, Ordering::Release);
        Ok(())
    }

    /// Loads disk-backed stem paths. Each file is decoded on its own producer
    /// thread and consumed through a bounded ring buffer by the output callback.
    pub fn load_streaming_session(
        &self,
        stems: Vec<(StemType, PathBuf)>,
    ) -> Result<(), DomainError> {
        if stems.len() != TRACK_COUNT {
            return Err(DomainError::UnexpectedStemCount(stems.len()));
        }
        let mut sorted = stems;
        sorted.sort_by_key(|(kind, _)| *kind);
        for pair in sorted.windows(2) {
            if pair[0].0 == pair[1].0 {
                return Err(DomainError::DuplicateStem(pair[0].0));
            }
        }
        for kind in StemType::ALL {
            if !sorted.iter().any(|(stem_kind, _)| *stem_kind == kind) {
                return Err(DomainError::MissingStem(kind));
            }
        }
        self.stop()?;
        *self
            .session
            .lock()
            .map_err(|_| DomainError::DspFailure("audio session state was poisoned".into()))? =
            Some(PreparedSession {
                playback: PlaybackSession::Streaming(Arc::from(sorted)),
            });
        Ok(())
    }
}

impl Drop for CpalRealtimeMixer {
    fn drop(&mut self) {
        let _ = self.commands.send(MixerCommand::Shutdown);
        if let Ok(worker) = self.worker.get_mut()
            && let Some(worker) = worker.take()
        {
            let _ = worker.join();
        }
    }
}

impl RealtimePlaybackPort for CpalRealtimeMixer {
    fn load_session(&self, stems: Vec<Stem>) -> Result<(), DomainError> {
        if stems.len() != TRACK_COUNT {
            return Err(DomainError::UnexpectedStemCount(stems.len()));
        }
        let mut sorted = stems;
        sorted.sort_by_key(|stem| stem.kind);
        for pair in sorted.windows(2) {
            if pair[0].kind == pair[1].kind {
                return Err(DomainError::DuplicateStem(pair[0].kind));
            }
        }
        for kind in StemType::ALL {
            if !sorted.iter().any(|stem| stem.kind == kind) {
                return Err(DomainError::MissingStem(kind));
            }
        }
        let format = sorted[0].audio.format();
        if let Some(stem) = sorted.iter().find(|stem| {
            stem.audio.format() != format || stem.audio.frames() != sorted[0].audio.frames()
        }) {
            return Err(DomainError::StemFormatMismatch(stem.kind));
        }
        self.stop()?;
        *self
            .session
            .lock()
            .map_err(|_| DomainError::DspFailure("audio session state was poisoned".into()))? =
            Some(PreparedSession {
                playback: PlaybackSession::Memory(Arc::from(sorted)),
            });
        Ok(())
    }

    fn set_track_state(&self, stem_id: StemType, gain_db: f32, pan: f32, mute: bool, solo: bool) {
        if !gain_db.is_finite() || !pan.is_finite() {
            return;
        }
        let Some(index) = track_index(stem_id) else {
            return;
        };
        let state = &self.controls[index];
        state.gain.store(
            10.0f32.powf(gain_db.clamp(-96.0, 24.0) / 20.0).to_bits(),
            Ordering::Relaxed,
        );
        let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        state
            .pan_left
            .store(angle.cos().to_bits(), Ordering::Relaxed);
        state
            .pan_right
            .store(angle.sin().to_bits(), Ordering::Relaxed);
        state.mute.store(mute, Ordering::Relaxed);
        state.solo.store(solo, Ordering::Relaxed);
    }

    fn set_pitch_semitones(
        &self,
        semitones: f32,
        preserve_formants: bool,
    ) -> Result<(), DomainError> {
        PitchShiftSpec::try_new(semitones, preserve_formants)?;
        if preserve_formants {
            return Err(DomainError::DspFailure(
                "formant-preserving pitch shifting is not implemented by the realtime playback-rate processor".into(),
            ));
        }
        self.pitch_ratio
            .store(2.0f32.powf(semitones / 12.0).to_bits(), Ordering::Relaxed);
        Ok(())
    }
}

fn run_device_worker(receiver: mpsc::Receiver<MixerCommand>) {
    let mut active: Option<ActiveStream> = None;
    while let Ok(command) = receiver.recv() {
        match command {
            MixerCommand::Start {
                session,
                controls,
                pitch_ratio,
                reply,
            } => {
                let result = if active.is_some() {
                    Ok(())
                } else {
                    build_stream(session, controls, pitch_ratio).map(|built| active = Some(built))
                };
                let _ = reply.send(result);
            }
            MixerCommand::Stop(reply) => {
                drop_active(active.take());
                let _ = reply.send(());
            }
            MixerCommand::Shutdown => {
                drop_active(active.take());
                break;
            }
        }
    }
}

struct ActiveStream {
    stream: cpal::Stream,
    _producers: Vec<StreamProducerGuard>,
}

fn drop_active(active: Option<ActiveStream>) {
    if let Some(ActiveStream { stream, _producers }) = active {
        drop(stream);
        drop(_producers);
    }
}

enum TrackReader {
    Memory {
        stems: Arc<[Stem]>,
        index: usize,
        cursor: f64,
    },
    Stream {
        consumer: rtrb::Consumer<f32>,
        channels: usize,
        sample_rate_hz: u32,
        current: Option<[f32; 2]>,
        next: Option<[f32; 2]>,
        fraction: f64,
        pending_sample: Option<f32>,
    },
}

impl TrackReader {
    fn frame(&mut self, step: f64) -> Option<[f32; 2]> {
        match self {
            Self::Memory {
                stems,
                index,
                cursor,
            } => {
                let audio = &stems[*index].audio;
                let channels = usize::from(audio.format().channels);
                let samples = audio.samples();
                let frames = audio.frames();
                let position = *cursor;
                *cursor += step;
                let source_frame = position as usize;
                if source_frame >= frames {
                    return None;
                }
                let next_frame = (source_frame + 1).min(frames - 1);
                let fraction = (position - source_frame as f64) as f32;
                let base = source_frame * channels;
                let next = next_frame * channels;
                let left = samples[base] + (samples[next] - samples[base]) * fraction;
                let right = if channels == 2 {
                    samples[base + 1] + (samples[next + 1] - samples[base + 1]) * fraction
                } else {
                    left
                };
                Some([left, right])
            }
            Self::Stream {
                consumer,
                channels,
                current,
                next,
                fraction,
                pending_sample,
                ..
            } => {
                if current.is_none() {
                    *current = pop_stream_frame(consumer, *channels, pending_sample);
                }
                if next.is_none() {
                    *next = pop_stream_frame(consumer, *channels, pending_sample);
                }
                let (Some(current_frame), Some(next_frame)) = (*current, *next) else {
                    return None;
                };
                let interpolated = [
                    current_frame[0] + (next_frame[0] - current_frame[0]) * *fraction as f32,
                    current_frame[1] + (next_frame[1] - current_frame[1]) * *fraction as f32,
                ];
                *fraction += step;
                while *fraction >= 1.0 {
                    *current = *next;
                    *next = pop_stream_frame(consumer, *channels, pending_sample);
                    *fraction -= 1.0;
                    if next.is_none() {
                        break;
                    }
                }
                Some(interpolated)
            }
        }
    }
}

fn pop_stream_frame(
    consumer: &mut rtrb::Consumer<f32>,
    channels: usize,
    pending_sample: &mut Option<f32>,
) -> Option<[f32; 2]> {
    let first = pending_sample.take().or_else(|| consumer.pop().ok())?;
    if channels == 1 {
        return Some([first, first]);
    }
    match consumer.pop() {
        Ok(second) => Some([first, second]),
        Err(_) => {
            *pending_sample = Some(first);
            None
        }
    }
}

fn build_stream(
    session: PlaybackSession,
    controls: Arc<MixerControls>,
    pitch_ratio: Arc<AtomicU32>,
) -> Result<ActiveStream, String> {
    let mut producers = Vec::new();
    let tracks = match session {
        PlaybackSession::Memory(stems) => {
            if stems.len() != TRACK_COUNT {
                return Err("loaded session does not contain exactly five stems".into());
            }
            (0..TRACK_COUNT)
                .map(|index| TrackReader::Memory {
                    stems: Arc::clone(&stems),
                    index,
                    cursor: 0.0,
                })
                .collect()
        }
        PlaybackSession::Streaming(paths) => {
            let mut tracks = Vec::with_capacity(TRACK_COUNT);
            let mut expected_format = None;
            for (_, path) in paths.iter() {
                let source =
                    BufferedStreamTrackSource::open(path).map_err(|error| error.to_string())?;
                let format = source.format();
                if let Some(expected) = expected_format
                    && expected != format
                {
                    return Err(
                        "streamed stems must have matching sample rates and channels".into(),
                    );
                }
                expected_format = Some(format);
                let (consumer, producer) = source.into_parts();
                producers.push(producer);
                tracks.push(TrackReader::Stream {
                    consumer,
                    channels: usize::from(format.channels),
                    sample_rate_hz: format.sample_rate_hz,
                    current: None,
                    next: None,
                    fraction: 0.0,
                    pending_sample: None,
                });
            }
            tracks
        }
    };
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no audio output device is available".to_owned())?;
    let supported = device
        .default_output_config()
        .map_err(|error| error.to_string())?;
    let output_channels = usize::from(supported.channels());
    if !(1..=2).contains(&output_channels) {
        return Err(format!(
            "output device has {output_channels} channels; mono and stereo are supported"
        ));
    }
    let output_rate = supported.sample_rate().0;
    let config = supported.config();
    match supported.sample_format() {
        CpalSampleFormat::F32 => build_typed_stream::<f32>(
            &device,
            &config,
            output_channels,
            output_rate,
            tracks,
            controls,
            pitch_ratio,
        ),
        CpalSampleFormat::I16 => build_typed_stream::<i16>(
            &device,
            &config,
            output_channels,
            output_rate,
            tracks,
            controls,
            pitch_ratio,
        ),
        CpalSampleFormat::U16 => build_typed_stream::<u16>(
            &device,
            &config,
            output_channels,
            output_rate,
            tracks,
            controls,
            pitch_ratio,
        ),
        format => Err(format!(
            "default output sample format {format:?} is unsupported"
        )),
    }
    .map(|stream| ActiveStream {
        stream,
        _producers: producers,
    })
}

fn build_typed_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    output_channels: usize,
    output_rate: u32,
    mut tracks: Vec<TrackReader>,
    controls: Arc<MixerControls>,
    pitch_ratio: Arc<AtomicU32>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let stream = device
        .build_output_stream(
            config,
            move |output: &mut [T], _| {
                mix_output(
                    output,
                    output_channels,
                    output_rate,
                    &mut tracks,
                    &controls,
                    &pitch_ratio,
                );
            },
            |_| {},
            None,
        )
        .map_err(|error| error.to_string())?;
    stream.play().map_err(|error| error.to_string())?;
    Ok(stream)
}

fn mix_output<T>(
    output: &mut [T],
    output_channels: usize,
    output_rate: u32,
    tracks: &mut [TrackReader],
    controls: &MixerControls,
    pitch_ratio: &AtomicU32,
) where
    T: Sample + FromSample<f32>,
{
    if !controls.transport.playing.load(Ordering::Acquire) {
        output.fill(T::from_sample(0.0));
        controls.transport.peak.store(0, Ordering::Relaxed);
        return;
    }
    let initial_position = controls.transport.position.load(Ordering::Acquire);
    let initial_seconds = f64::from_bits(initial_position);
    for track in tracks.iter_mut() {
        if let TrackReader::Memory { stems, cursor, .. } = track {
            *cursor = initial_seconds * f64::from(stems[0].audio.format().sample_rate_hz);
        }
    }
    let mut peak = 0.0f32;
    let input_rate = match tracks.first() {
        Some(TrackReader::Memory { stems, .. }) => stems[0].audio.format().sample_rate_hz,
        Some(TrackReader::Stream { sample_rate_hz, .. }) => *sample_rate_hz,
        None => output_rate,
    };
    let rate_step = f64::from(input_rate) / f64::from(output_rate)
        * f64::from(f32::from_bits(pitch_ratio.load(Ordering::Relaxed)));
    let any_solo = controls
        .iter()
        .any(|control| control.solo.load(Ordering::Relaxed));
    let complete_sample_count = output.len() / output_channels * output_channels;
    for frame in output[..complete_sample_count].chunks_exact_mut(output_channels) {
        let mut left = 0.0f32;
        let mut right = 0.0f32;
        for (index, track) in tracks.iter_mut().enumerate() {
            let control = &controls[index];
            let sample = track.frame(rate_step);
            if control.mute.load(Ordering::Relaxed)
                || (any_solo && !control.solo.load(Ordering::Relaxed))
            {
                continue;
            }
            let Some([sample_left, sample_right]) = sample else {
                continue;
            };
            let gain = f32::from_bits(control.gain.load(Ordering::Relaxed));
            let pan_left = f32::from_bits(control.pan_left.load(Ordering::Relaxed));
            let pan_right = f32::from_bits(control.pan_right.load(Ordering::Relaxed));
            left += sample_left * gain * pan_left;
            right += sample_right * gain * pan_right;
        }
        let master = f32::from_bits(controls.transport.master.load(Ordering::Relaxed));
        // Apply master before clipping, retaining headroom when summing stems.
        left *= master;
        right *= master;
        peak = peak.max(left.abs()).max(right.abs());
        if output_channels == 1 {
            frame[0] =
                T::from_sample(((left + right) * std::f32::consts::FRAC_1_SQRT_2).clamp(-1.0, 1.0));
        } else {
            frame[0] = T::from_sample(left.clamp(-1.0, 1.0));
            frame[1] = T::from_sample(right.clamp(-1.0, 1.0));
        }
    }
    output[complete_sample_count..].fill(T::from_sample(0.0));
    controls
        .transport
        .peak
        .store(peak.to_bits(), Ordering::Relaxed);
    if let Some(TrackReader::Memory { stems, cursor, .. }) = tracks.first() {
        let audio = &stems[0].audio;
        let seconds = cursor.min(audio.frames() as f64) / f64::from(audio.format().sample_rate_hz);
        let _ = controls.transport.position.compare_exchange(
            initial_position,
            seconds.to_bits(),
            Ordering::AcqRel,
            Ordering::Relaxed,
        );
        if *cursor >= audio.frames() as f64 {
            controls.transport.playing.store(false, Ordering::Release);
        }
    }
}

fn track_index(kind: StemType) -> Option<usize> {
    StemType::ALL
        .iter()
        .position(|candidate| *candidate == kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use atomusicator_domain::{AudioBufferF32, AudioFormat};

    fn stems(value: f32) -> Vec<Stem> {
        let format = AudioFormat::new(48_000, 2).unwrap();
        StemType::ALL
            .into_iter()
            .map(|kind| Stem {
                kind,
                audio: AudioBufferF32::new(format, vec![value; 4]).unwrap(),
            })
            .collect()
    }

    fn readers(stems: Vec<Stem>) -> Vec<TrackReader> {
        let stems: Arc<[Stem]> = Arc::from(stems);
        (0..TRACK_COUNT)
            .map(|index| TrackReader::Memory {
                stems: Arc::clone(&stems),
                index,
                cursor: 0.0,
            })
            .collect()
    }

    #[test]
    fn callback_mixes_tracks_applies_pan_mute_and_clips_output() {
        let mut tracks = readers(stems(0.5));
        let controls = MixerControls::new();
        for kind in StemType::ALL.into_iter().skip(1) {
            let control = &controls[track_index(kind).unwrap()];
            control.mute.store(true, Ordering::Relaxed);
        }
        let lead = &controls[track_index(StemType::LeadVocal).unwrap()];
        lead.pan_left.store(1.0f32.to_bits(), Ordering::Relaxed);
        lead.pan_right.store(0.0f32.to_bits(), Ordering::Relaxed);
        let pitch = AtomicU32::new(1.0f32.to_bits());
        let mut output = [0.0; 4];

        mix_output(&mut output, 2, 48_000, &mut tracks, &controls, &pitch);

        assert_eq!(output, [0.5, 0.0, 0.5, 0.0]);
    }

    #[test]
    fn callback_soloes_tracks_and_limiter_clamps_summed_signal() {
        let mut tracks = readers(stems(1.0));
        let controls = MixerControls::new();
        let bass = &controls[track_index(StemType::Bass).unwrap()];
        bass.solo.store(true, Ordering::Relaxed);
        bass.gain.store(2.0f32.to_bits(), Ordering::Relaxed);
        let pitch = AtomicU32::new(1.0f32.to_bits());
        let mut output = [0.0; 4];

        mix_output(&mut output, 2, 48_000, &mut tracks, &controls, &pitch);

        assert_eq!(output, [1.0; 4]);
    }

    #[test]
    fn callback_reads_streamed_frames_without_blocking() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(8);
        for sample in [0.25, -0.25, 0.5, -0.5, 0.75, -0.75] {
            producer.push(sample).unwrap();
        }
        let mut track = TrackReader::Stream {
            consumer,
            channels: 2,
            sample_rate_hz: 48_000,
            current: None,
            next: None,
            fraction: 0.0,
            pending_sample: None,
        };

        assert_eq!(track.frame(1.0), Some([0.25, -0.25]));
        assert_eq!(track.frame(1.0), Some([0.5, -0.5]));
    }

    #[test]
    fn pitch_rate_rejects_unsupported_formant_preservation() {
        let mixer = CpalRealtimeMixer::new().unwrap();
        assert!(mixer.set_pitch_semitones(2.0, false).is_ok());
        assert!(mixer.set_pitch_semitones(2.0, true).is_err());
        assert!(mixer.set_pitch_semitones(13.0, false).is_err());
    }

    #[test]
    fn loading_rejects_tracks_with_different_lengths() {
        let mixer = CpalRealtimeMixer::new().unwrap();
        let mut tracks = stems(0.1);
        tracks[1].audio = AudioBufferF32::new(tracks[1].audio.format(), vec![0.1; 6]).unwrap();
        assert!(mixer.load_session(tracks).is_err());
    }

    #[test]
    fn transport_pauses_seeks_and_applies_master_gain_to_real_samples() {
        let mut tracks = readers(stems(0.4));
        let controls = MixerControls::new();
        controls.transport.playing.store(false, Ordering::Relaxed);
        let pitch = AtomicU32::new(1.0f32.to_bits());
        let mut output = [1.0f32; 2];
        mix_output(&mut output, 2, 48000, &mut tracks, &controls, &pitch);
        assert_eq!(output, [0.0; 2]);
        assert_eq!(controls.transport.position.load(Ordering::Relaxed), 0);
        controls
            .transport
            .master
            .store(0.1f32.to_bits(), Ordering::Relaxed);
        controls.transport.playing.store(true, Ordering::Relaxed);
        mix_output(&mut output, 2, 48000, &mut tracks, &controls, &pitch);
        assert!((output[0] - 0.14142136).abs() < 1e-6);
        assert!(f64::from_bits(controls.transport.position.load(Ordering::Relaxed)) > 0.0);
        assert!(f32::from_bits(controls.transport.peak.load(Ordering::Relaxed)) > 0.0);
    }
}
