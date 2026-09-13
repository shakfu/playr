//! Device output: rate negotiation, the ring buffer, and the realtime callback.
//!
//! Rate negotiation is the part that matters for fidelity. The stream is opened
//! at the file's own sample rate whenever the device offers it, so a 44.1kHz
//! track reaches the device without passing through a resampler at all.

use std::any::Any;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    Device, ErrorKind, SampleFormat, StreamConfig, SupportedStreamConfig,
    SupportedStreamConfigRange,
};

use super::decode::Spec;

/// How much audio the ring holds. Two seconds is enough to ride out scheduler
/// jitter and a slow disk without making seek feel laggy.
const BUFFER_SECONDS: u32 = 2;

#[derive(Debug)]
pub enum OutputError {
    NoDevice,
    NoConfig,
    Build(String),
}

impl std::fmt::Display for OutputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputError::NoDevice => write!(f, "no audio output device"),
            OutputError::NoConfig => write!(f, "device offers no usable output format"),
            OutputError::Build(s) => write!(f, "could not open audio stream: {s}"),
        }
    }
}

impl std::error::Error for OutputError {}

/// What a device reports while its stream runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceEvent {
    /// The stream cannot continue: the device is gone, or the stream must be rebuilt.
    Lost(String),
    /// The system moved the stream to another device, and it keeps playing.
    Rerouted,
    /// Any other error; the stream may still be playing.
    Error(String),
}

impl From<cpal::Error> for DeviceEvent {
    fn from(e: cpal::Error) -> Self {
        match e.kind() {
            ErrorKind::DeviceChanged => DeviceEvent::Rerouted,
            ErrorKind::DeviceNotAvailable
            | ErrorKind::HostUnavailable
            | ErrorKind::StreamInvalidated => DeviceEvent::Lost(e.to_string()),
            _ => DeviceEvent::Error(e.to_string()),
        }
    }
}

/// An output device the engine plays to.
///
/// The engine owns the ring buffer and the pause state; a backend only chooses
/// a format and runs a stream that drains the ring. [`Cpal`] is the real one.
/// Tests supply a fake, which can fail on demand.
pub trait Backend: Send + 'static {
    /// Chooses an output format for `src`.
    fn negotiate(&self, src: Spec) -> Result<Plan, OutputError>;

    /// Starts a stream per `plan` that plays `consumer` through [`render`],
    /// sending device events to `events`. Dropping the result stops it.
    fn start(
        &self,
        plan: Plan,
        consumer: rtrb::Consumer<f32>,
        shared: Arc<Shared>,
        events: Sender<DeviceEvent>,
    ) -> Result<Box<dyn Any>, OutputError>;
}

/// A cpal output device.
pub struct Cpal(pub Device);

impl Backend for Cpal {
    fn negotiate(&self, src: Spec) -> Result<Plan, OutputError> {
        negotiate(&self.0, src)
    }

    fn start(
        &self,
        plan: Plan,
        consumer: rtrb::Consumer<f32>,
        shared: Arc<Shared>,
        events: Sender<DeviceEvent>,
    ) -> Result<Box<dyn Any>, OutputError> {
        let config = StreamConfig {
            channels: plan.channels,
            sample_rate: plan.rate,
            buffer_size: cpal::BufferSize::Default,
        };
        let device = &self.0;
        let stream = match plan.format {
            SampleFormat::F32 => build::<f32>(device, &config, consumer, shared, events, |v| v),
            SampleFormat::F64 => {
                build::<f64>(device, &config, consumer, shared, events, |v| v as f64)
            }
            // Scaled in f64 by the positive maximum, so full scale cannot wrap.
            SampleFormat::I32 => build::<i32>(device, &config, consumer, shared, events, |v| {
                (v.clamp(-1.0, 1.0) as f64 * i32::MAX as f64) as i32
            }),
            SampleFormat::I24 => {
                build::<cpal::I24>(device, &config, consumer, shared, events, |v| {
                    cpal::I24::new_unchecked((v.clamp(-1.0, 1.0) as f64 * 8_388_607.0) as i32)
                })
            }
            SampleFormat::I16 => build::<i16>(device, &config, consumer, shared, events, |v| {
                (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
            }),
            SampleFormat::U16 => build::<u16>(device, &config, consumer, shared, events, |v| {
                ((v.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16
            }),
            other => {
                return Err(OutputError::Build(format!(
                    "unsupported sample format {other:?}"
                )))
            }
        }?;
        // The device runs continuously; silence is produced in the callback
        // when paused. See `Shared::paused`.
        stream
            .play()
            .map_err(|e| OutputError::Build(e.to_string()))?;
        Ok(Box::new(stream))
    }
}

/// The format the device will actually be opened in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub rate: u32,
    pub channels: u16,
    pub format: SampleFormat,
}

impl Plan {
    /// True when the source must be rate-converted to reach the device.
    pub fn needs_resample(&self, src: Spec) -> bool {
        src.rate != 0 && src.rate != self.rate
    }
}

/// Chooses an output format for `src` on `device`. See [`choose`].
pub fn negotiate(device: &Device, src: Spec) -> Result<Plan, OutputError> {
    let ranges: Vec<SupportedStreamConfigRange> = device
        .supported_output_configs()
        .map_err(|_| OutputError::NoConfig)?
        .collect();
    choose(&ranges, device.default_output_config().ok(), src).ok_or(OutputError::NoConfig)
}

/// Chooses an output format for `src`, preferring the source's own sample rate.
///
/// Preference order: exact rate and channel count, then exact rate with a
/// different channel count, then the device default. Sample rate is ranked
/// above channel count because resampling colours the signal while channel
/// remapping does not. Returns `None` when no offered format can be written.
pub fn choose(
    ranges: &[SupportedStreamConfigRange],
    default: Option<SupportedStreamConfig>,
    src: Spec,
) -> Option<Plan> {
    let want_ch = if src.channels == 0 { 2 } else { src.channels };
    let supports = |r: &SupportedStreamConfigRange, rate: u32| {
        r.min_sample_rate() <= rate && rate <= r.max_sample_rate()
    };
    let usable: Vec<_> = ranges
        .iter()
        .filter_map(|r| format_rank(r.sample_format()).map(|rank| (rank, r)))
        .collect();
    let plan = |r: &SupportedStreamConfigRange, rate| Plan {
        rate,
        channels: r.channels(),
        format: r.sample_format(),
    };

    if src.rate != 0 {
        // Exact rate, exact channels.
        let exact = usable
            .iter()
            .filter(|(_, r)| r.channels() == want_ch && supports(r, src.rate))
            .min_by_key(|(rank, _)| *rank);
        if let Some((_, r)) = exact {
            return Some(plan(r, src.rate));
        }

        // Exact rate, any channel count. Remapping channels is lossless enough
        // to be preferable to resampling.
        let by_rate = usable
            .iter()
            .filter(|(_, r)| supports(r, src.rate))
            .min_by_key(|(rank, r)| (*rank, r.channels().abs_diff(want_ch)));
        if let Some((_, r)) = by_rate {
            return Some(plan(r, src.rate));
        }
    }

    let default = default?;
    if format_rank(default.sample_format()).is_some() {
        return Some(Plan {
            rate: default.sample_rate(),
            channels: default.channels(),
            format: default.sample_format(),
        });
    }
    // The default format cannot be written; keep its rate in one that can.
    usable
        .iter()
        .filter(|(_, r)| supports(r, default.sample_rate()))
        .min_by_key(|(rank, r)| (*rank, r.channels().abs_diff(default.channels())))
        .map(|(_, r)| plan(r, default.sample_rate()))
}

/// Rank of a format the output can write, best first; `None` if it cannot.
///
/// Floats first: the decoder produces f32, so they need no quantisation.
/// Integers follow by width, widest first.
fn format_rank(f: SampleFormat) -> Option<u8> {
    match f {
        SampleFormat::F32 => Some(0),
        SampleFormat::F64 => Some(1),
        SampleFormat::I32 => Some(2),
        SampleFormat::I24 => Some(3),
        SampleFormat::I16 => Some(4),
        SampleFormat::U16 => Some(5),
        _ => None,
    }
}

pub fn default_device() -> Result<Device, OutputError> {
    cpal::default_host()
        .default_output_device()
        .ok_or(OutputError::NoDevice)
}

/// State the realtime callback shares with the rest of the player.
///
/// All of it is atomic: the callback must never take a lock.
pub struct Shared {
    /// Frames handed to the device since the last reset. This is the audible
    /// position, as distinct from how far the decoder has read ahead.
    pub frames_out: AtomicU64,
    /// Playback gain, as f32 bits.
    volume: AtomicU32,
    /// When set, the callback emits silence and consumes nothing.
    ///
    /// Pausing is done here rather than with `Stream::pause` because ALSA
    /// devices commonly reject `snd_pcm_pause` (errno 77), and stopping the
    /// device also risks a click on resume.
    pub paused: AtomicBool,
    /// Output sample rate, for converting `frames_out` into a time.
    pub position_rate: AtomicU32,
    /// `frames_out` value at which the current track began.
    pub track_start: AtomicU64,
    /// Frames to add to the position, set when seeking.
    ///
    /// A seek resets `frames_out` to zero while playback is actually partway
    /// into the track. This carries the difference.
    /// It is an addition rather than a negative `track_start` because these are
    /// unsigned: a wrapped subtraction reads back as a clamp to zero.
    pub position_offset: AtomicU64,
    /// Playback speed, as f32 bits, for turning device time into track time.
    speed_bits: AtomicU32,
    /// Raised by the engine to have the callback discard everything buffered.
    pub flush_requested: AtomicU64,
    /// The last `flush_requested` value the callback has acted on.
    pub flush_done: AtomicU64,
}

impl Shared {
    pub fn new() -> Self {
        Shared {
            frames_out: AtomicU64::new(0),
            volume: AtomicU32::new(1.0f32.to_bits()),
            paused: AtomicBool::new(false),
            position_rate: AtomicU32::new(0),
            track_start: AtomicU64::new(0),
            position_offset: AtomicU64::new(0),
            speed_bits: AtomicU32::new(1.0f32.to_bits()),
            flush_requested: AtomicU64::new(0),
            flush_done: AtomicU64::new(0),
        }
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    pub fn set_volume(&self, v: f32) {
        self.volume
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn speed(&self) -> f64 {
        f32::from_bits(self.speed_bits.load(Ordering::Relaxed)) as f64
    }

    pub fn set_speed(&self, v: f64) {
        self.speed_bits
            .store((v as f32).to_bits(), Ordering::Relaxed);
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

/// An open output stream and the producer end of its ring buffer.
pub struct Output {
    _stream: Box<dyn Any>,
    pub producer: rtrb::Producer<f32>,
    pub plan: Plan,
    pub capacity: usize,
    pub shared: Arc<Shared>,
}

impl Output {
    /// Opens a stream on `backend` per `plan`, with a two second ring.
    ///
    /// It plays or stays silent as `shared.paused` already says.
    pub fn open(
        backend: &dyn Backend,
        plan: Plan,
        shared: Arc<Shared>,
        events: Sender<DeviceEvent>,
    ) -> Result<Self, OutputError> {
        let capacity = (plan.rate * BUFFER_SECONDS) as usize * plan.channels as usize;
        let (producer, consumer) = rtrb::RingBuffer::<f32>::new(capacity);
        let stream = backend.start(plan, consumer, shared.clone(), events)?;
        Ok(Output {
            _stream: stream,
            producer,
            plan,
            capacity,
            shared,
        })
    }

    pub fn play(&self) {
        self.shared.paused.store(false, Ordering::Relaxed);
    }

    pub fn pause(&self) {
        self.shared.paused.store(true, Ordering::Relaxed);
    }

    /// Frames currently sitting in the ring, not yet played.
    pub fn buffered_frames(&self) -> usize {
        (self.capacity - self.producer.slots()) / self.plan.channels as usize
    }

    pub fn is_drained(&self) -> bool {
        self.buffered_frames() == 0
    }
}

/// Builds a cpal stream. `conv` maps a gain-applied f32 sample to the device type.
fn build<T>(
    device: &Device,
    config: &StreamConfig,
    mut consumer: rtrb::Consumer<f32>,
    shared: Arc<Shared>,
    events: Sender<DeviceEvent>,
    conv: fn(f32) -> T,
) -> Result<cpal::Stream, OutputError>
where
    T: cpal::SizedSample + Send + 'static,
{
    let channels = config.channels as u64;
    device
        .build_output_stream::<T, _, _>(
            *config,
            move |out: &mut [T], _| render(out, &mut consumer, &shared, channels, conv),
            // Printing here would draw over the interface.
            move |e| {
                let _ = events.send(e.into());
            },
            None,
        )
        .map_err(|e| OutputError::Build(e.to_string()))
}

/// Fills `out` from the ring, and counts the frames played.
///
/// The body of every output callback, so it must not lock or allocate. It
/// emits silence while paused, and on underrun rather than repeating stale
/// samples, which would click.
pub fn render<T>(
    out: &mut [T],
    consumer: &mut rtrb::Consumer<f32>,
    shared: &Shared,
    channels: u64,
    conv: fn(f32) -> T,
) {
    // Before the pause check, so a seek while paused still discards.
    let requested = shared.flush_requested.load(Ordering::Relaxed);
    if requested != shared.flush_done.load(Ordering::Relaxed) {
        if let Ok(chunk) = consumer.read_chunk(consumer.slots()) {
            chunk.commit_all();
        }
        shared.flush_done.store(requested, Ordering::Relaxed);
    }
    if shared.paused.load(Ordering::Relaxed) {
        for slot in out.iter_mut() {
            *slot = conv(0.0);
        }
        return;
    }
    let gain = shared.volume();
    let mut filled = 0usize;
    for slot in out.iter_mut() {
        match consumer.pop() {
            Ok(s) => {
                *slot = conv(s * gain);
                filled += 1;
            }
            Err(_) => *slot = conv(0.0),
        }
    }
    shared
        .frames_out
        .fetch_add(filled as u64 / channels, Ordering::Relaxed);
}

/// Maps interleaved audio from `src_ch` channels to `dst_ch`, appending to `out`.
///
/// Mono fans out to every channel; extra source channels beyond the device's
/// count are dropped. Downmixing properly would need per-layout coefficients,
/// which is not worth it until something actually plays 5.1.
pub fn remap_channels(input: &[f32], src_ch: usize, dst_ch: usize, out: &mut Vec<f32>) {
    if src_ch == dst_ch {
        out.extend_from_slice(input);
        return;
    }
    if src_ch == 0 || dst_ch == 0 {
        return;
    }
    for frame in input.chunks_exact(src_ch) {
        if src_ch == 1 {
            out.extend(std::iter::repeat_n(frame[0], dst_ch));
        } else {
            for c in 0..dst_ch {
                out.push(frame.get(c).copied().unwrap_or(0.0));
            }
        }
    }
}
