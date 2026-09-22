//! Symphonia wrapper: a file becomes a stream of interleaved f32 frames.
//!
//! This module is the seam for format support. Everything above it sees only
//! `Spec` and `&[f32]`, so adding a decoder Symphonia lacks (Opus, WavPack)
//! means adding a variant here, not changing the player.

use std::fs::File;
use std::path::Path;
use std::time::Duration;

use symphonia::core::codecs::audio::well_known::{
    CODEC_ID_AAC, CODEC_ID_ALAC, CODEC_ID_FLAC, CODEC_ID_OPUS, CODEC_ID_PCM_F64BE_PLANAR,
    CODEC_ID_PCM_S32LE, CODEC_ID_WAVPACK,
};
use symphonia::core::codecs::audio::{
    AudioCodecId, AudioDecoder, AudioDecoderOptions, VerificationCheck,
};
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymphError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase};

/// Symphonia's built-in decoders plus the ones playr adds.
///
/// Built once: constructing a registry walks every codec, and a track change
/// would otherwise pay for it again.
fn codecs() -> &'static CodecRegistry {
    static REGISTRY: std::sync::OnceLock<CodecRegistry> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut registry);
        #[cfg(feature = "opus")]
        registry.register_audio_decoder::<crate::audio::opus::OpusDecoder>();
        registry
    })
}

/// Sample rate and channel count of a decoded stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub rate: u32,
    pub channels: u16,
}

#[derive(Debug)]
pub enum DecodeError {
    Io(std::io::Error),
    /// The container parsed but no decoder is registered for its codec.
    /// Opus and WavPack land here: Symphonia 0.6 demuxes OGG but cannot decode Opus.
    Unsupported(String),
    Other(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Io(e) => write!(f, "io: {e}"),
            // Symphonia's own text ("core (codec): unsupported audio codec")
            // says nothing a listener can act on, so point at `playr formats`.
            DecodeError::Unsupported(_) => {
                write!(f, "no decoder for this format (see: playr formats)")
            }
            DecodeError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl From<SymphError> for DecodeError {
    fn from(e: SymphError) -> Self {
        match e {
            SymphError::IoError(e) => DecodeError::Io(e),
            SymphError::Unsupported(s) => DecodeError::Unsupported(s.to_string()),
            other => DecodeError::Other(other.to_string()),
        }
    }
}

/// A decoded audio file, pulled one packet at a time.
pub struct AudioStream {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    spec: Spec,
    duration: Option<Duration>,
    /// Scratch for the interleaved f32 conversion, reused across packets.
    buf: Vec<f32>,
    /// Decoded frames still to drop after a seek.
    ///
    /// An accurate seek lands on a packet at or before the target, and the
    /// caller must discard up to the target itself.
    discard: usize,
    /// Added to a seek target to reach the container's timestamps.
    seek_offset: Duration,
    /// How far before a seek target decoding starts, so the decoder has
    /// settled by the target. See `pre_roll`.
    pre_roll: Duration,
    codec: AudioCodecId,
    /// Bits per sample the header gives, for integer formats.
    bits: Option<u32>,
    /// Frames the header says the stream holds.
    header_frames: Option<u64>,
    /// The MD5 of the decoded audio a FLAC header gives; `None` when it is
    /// absent, which FLAC writes as all zeros.
    md5: Option<[u8; 16]>,
    /// Packets that failed to decode and were skipped.
    skipped: u64,
}

/// Decoding time a codec needs after a reset before its output is exact.
///
/// Measured against decoding from the start. Opus is still 26 dB off 80 ms
/// in, the minimum RFC 7845 section 4.6 gives, and 98 dB off after 320 ms.
/// AAC is off for its first frame, which overlaps the one before it.
fn pre_roll(codec: symphonia::core::codecs::audio::AudioCodecId, rate: u32) -> Duration {
    match codec {
        CODEC_ID_OPUS => Duration::from_millis(320),
        CODEC_ID_AAC if rate > 0 => Duration::from_secs_f64(2048.0 / rate as f64),
        _ => Duration::ZERO,
    }
}

impl AudioStream {
    /// Opens `path` and prepares a decoder for its default audio track.
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        Self::open_with(path, false)
    }

    /// As [`AudioStream::open`], with the decoder checking the decoded audio
    /// against the header's checksum, which [`AudioStream::finalize`] reports.
    pub fn open_verifying(path: &Path) -> Result<Self, DecodeError> {
        Self::open_with(path, true)
    }

    fn open_with(path: &Path, verify: bool) -> Result<Self, DecodeError> {
        let file = File::open(path).map_err(DecodeError::Io)?;
        let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let reader = symphonia::default::get_probe().probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )?;

        let track = reader
            .first_track_known_codec(TrackType::Audio)
            .ok_or_else(|| DecodeError::Unsupported("no decodable audio track".into()))?;

        let track_id = track.id;
        let time_base = track.time_base;
        let header_frames = track.num_frames;
        let duration = track
            .duration
            .zip(time_base)
            .and_then(|(d, tb)| tb.calc_duration(d))
            .map(|t| Duration::from_secs_f64(t.as_secs_f64().max(0.0)));

        let Some(CodecParameters::Audio(params)) = track.codec_params.clone() else {
            return Err(DecodeError::Unsupported(
                "track has no audio codec parameters".into(),
            ));
        };

        let options = AudioDecoderOptions::default().verify(verify);
        let decoder = codecs().make_audio_decoder(&params, &options)?;
        let md5 = match params.verification_check {
            Some(VerificationCheck::Md5(md5)) => Some(md5),
            _ => None,
        };

        // OGG timestamps count Opus pre-skip, which the decoder trims, not
        // the demuxer. Matroska subtracts it already and reports no delay.
        let seek_offset = match (params.codec, track.delay) {
            // Pre-skip is counted in 48 kHz samples whatever the output rate.
            (CODEC_ID_OPUS, Some(delay)) => Duration::from_secs_f64(delay as f64 / 48_000.0),
            _ => Duration::ZERO,
        };
        let pre_roll = pre_roll(params.codec, params.sample_rate.unwrap_or(0));

        // Rate and channels are not always in the container header; when absent
        // they are filled in from the first decoded buffer.
        let spec = Spec {
            rate: params.sample_rate.unwrap_or(0),
            channels: params
                .channels
                .as_ref()
                .map(|c| c.count() as u16)
                .unwrap_or(0),
        };

        Ok(AudioStream {
            reader,
            decoder,
            track_id,
            time_base,
            spec,
            duration,
            buf: Vec::new(),
            discard: 0,
            seek_offset,
            pre_roll,
            codec: params.codec,
            bits: params.bits_per_sample,
            header_frames,
            md5,
            skipped: 0,
        })
    }

    pub fn spec(&self) -> Spec {
        self.spec
    }

    pub fn duration(&self) -> Option<Duration> {
        self.duration
    }

    /// Whether the codec is lossless: FLAC, ALAC, WavPack or linear PCM.
    pub fn lossless(&self) -> bool {
        // Symphonia numbers linear PCM contiguously; A-law and mu-law follow it.
        matches!(self.codec, CODEC_ID_FLAC | CODEC_ID_ALAC | CODEC_ID_WAVPACK)
            || (CODEC_ID_PCM_S32LE..=CODEC_ID_PCM_F64BE_PLANAR).contains(&self.codec)
    }

    /// Whether the stream is FLAC, whose header may carry an MD5.
    pub fn is_flac(&self) -> bool {
        self.codec == CODEC_ID_FLAC
    }

    /// Bits per sample the header gives, for integer formats.
    pub fn bits(&self) -> Option<u32> {
        self.bits
    }

    /// Frames the header says the stream holds, when it says.
    pub fn header_frames(&self) -> Option<u64> {
        self.header_frames
    }

    /// The MD5 a FLAC header gives, when it is not all zeros.
    pub fn md5(&self) -> Option<[u8; 16]> {
        self.md5
    }

    /// Packets that failed to decode and were skipped so far.
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// Ends decoding. For a stream opened with [`AudioStream::open_verifying`],
    /// whether the decoded audio matched the header's checksum, when it has one.
    pub fn finalize(&mut self) -> Option<bool> {
        self.decoder.finalize().verify_ok
    }

    /// Decodes the next packet into interleaved f32. `Ok(None)` means end of stream.
    ///
    /// Packets that fail to decode are skipped rather than ending playback: a
    /// single corrupt frame in the middle of a file should not stop the track.
    pub fn next_chunk(&mut self) -> Result<Option<&[f32]>, DecodeError> {
        loop {
            let packet = match self.reader.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(None),
                Err(SymphError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(None)
                }
                Err(e) => return Err(e.into()),
            };

            if packet.track_id != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    // Fill in rate/channels the container omitted, from the
                    // first decoded buffer.
                    if self.spec.rate == 0 {
                        self.spec.rate = decoded.spec().rate();
                    }
                    if self.spec.channels == 0 {
                        self.spec.channels = decoded.spec().channels().count() as u16;
                    }
                    if decoded.frames() == 0 {
                        continue;
                    }
                    // Encoder delay and padding are already trimmed by the
                    // demuxers that support gapless (FLAC, MP3, Vorbis, WAV,
                    // AIFF, ALAC). Re-applying `packet.trim_*` here double-trims
                    // them. AAC has no gapless support upstream and decodes
                    // ~1900 frames long; that is a known limitation.
                    decoded.copy_to_vec_interleaved(&mut self.buf);
                    let ch = decoded.spec().channels().count().max(1);
                    let drop = self.discard.min(decoded.frames());
                    self.discard -= drop;
                    if drop == decoded.frames() {
                        continue;
                    }
                    return Ok(Some(&self.buf[drop * ch..]));
                }
                // Recoverable per the Symphonia contract: skip and keep going.
                Err(SymphError::DecodeError(_)) => {
                    self.skipped += 1;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Seeks to `pos` from the start of the track.
    pub fn seek(&mut self, pos: Duration) -> Result<(), DecodeError> {
        let target = pos + self.seek_offset;
        let from = target.saturating_sub(self.pre_roll);
        let time = Time::try_new(from.as_secs() as i64, from.subsec_nanos())
            .ok_or_else(|| DecodeError::Other("seek position out of range".into()))?;
        let seeked = self.reader.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time,
                track_id: Some(self.track_id),
            },
        )?;
        // The decoder holds state from before the seek; discarding it prevents
        // a burst of garbage frames at the new position.
        self.decoder.reset();
        let secs = |ts| {
            self.time_base
                .and_then(|tb| tb.calc_time(ts))
                .map(|t| t.as_secs_f64())
        };
        // Up to the target, which covers the pre-roll as well as the packet.
        self.discard = match secs(seeked.actual_ts) {
            Some(act) => {
                ((target.as_secs_f64() - act).max(0.0) * self.spec.rate as f64).round() as usize
            }
            None => 0,
        };
        Ok(())
    }
}
